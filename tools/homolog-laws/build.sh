#!/usr/bin/env bash
# Rebuilds catalog/br/homologacao/leis.json from tools/homolog-laws/curated.json and the
# official sources. Fails explicitly (exit 1, nothing written) if any source is
# unreachable or any item cannot be verified; it never drops items silently.
#
#   build.sh build [--date YYYY-MM-DD] [--out FILE]   fetch, verify, write the dataset
#   build.sh validate [FILE]                          offline schema checks of a dataset
#   build.sh verify URL TITLE [NUMBER] [YEAR]         check one page (curation helper)
#
# Requires: bash, curl, jq, xmllint, iconv, pdftotext.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
curated="$here/curated.json"
catalog="$root/catalog/br/jurisdictions.csv"
default_out="$root/catalog/br/homologacao/leis.json"
ua="Mozilla/5.0 (X11; Linux x86_64) dwc-homolog-laws/1.0"
senado_api="https://legis.senado.leg.br/dadosabertos/legislacao/lista"

die() { echo "error: $*" >&2; exit 1; }
for tool in curl jq xmllint iconv pdftotext; do
    command -v "$tool" >/dev/null || die "missing tool: $tool"
done

# Lowercase ASCII words separated by single spaces (accents, tags and punctuation removed).
normalize() {
    iconv -f utf-8 -t ascii//TRANSLIT 2>/dev/null | tr 'A-Z' 'a-z' | tr -c 'a-z0-9' ' ' | tr -s ' '
}

# Only HTTPS official Brazilian government/legislative hosts.
official_url() {
    [[ "$1" =~ ^https://([a-z0-9-]+\.)+(gov\.br|leg\.br|jus\.br)(/|$) ]]
}

# fetch URL -> normalized page text on stdout; non-zero exit on any failure.
fetch_text() {
    local url="$1" tmp ctype charset
    tmp="$(mktemp)"
    if ! ctype="$(curl -fsSL --proto '=https' --retry 4 --retry-all-errors --retry-delay 5 -m 120 -A "$ua" \
        -o "$tmp" -w '%{content_type}' "$url")"; then
        rm -f "$tmp"
        return 1
    fi
    if [[ "$ctype" == *pdf* ]] || head -c 5 "$tmp" | grep -q '%PDF-'; then
        pdftotext -q -enc UTF-8 "$tmp" - | normalize
    else
        # The HTTP charset wins over <meta>: convert to UTF-8 and declare it.
        charset="$(grep -io 'charset=[a-z0-9_-]*' <<<"$ctype" | head -n 1 | cut -d= -f2 || true)"
        # A UTF-16 byte-order mark (Word-exported pages) wins over both.
        case "$(head -c 2 "$tmp" | od -An -tx1 | tr -d ' \n')" in
            fffe | feff) charset="UTF-16" ;;
        esac
        if [[ -n "$charset" ]]; then
            iconv -c -f "$charset" -t UTF-8 "$tmp" 2>/dev/null \
                | sed -E 's/<meta[^>]*charset[^>]*>//Ig; 1s/^/<meta http-equiv="Content-Type" content="text\/html; charset=utf-8">/' \
                >"$tmp.html" || true
            mv "$tmp.html" "$tmp"
        fi
        # A space before every tag keeps adjacent text nodes apart ("2022</p><p>Dispõe").
        sed 's/</ </g' "$tmp" | xmllint --html --nowarning --xpath 'string(/)' - 2>/dev/null | normalize
    fi
    rm -f "$tmp"
}

# number_present TEXT DIGITS: digits appear raw ("8078") or grouped ("8 078").
number_present() {
    local text="$1" n="$2" grouped
    grouped="$(sed -E ':a;s/([0-9])([0-9]{3})( |$)/\1 \2\3/;ta' <<<"$n")"
    [[ " $text " == *" $n "* || " $text " == *" $grouped "* ]]
}

# check_page URL TITLE NUMBER YEAR -> prints "" on success, else the reason.
check_page() {
    local url="$1" title="$2" number="$3" year="$4" text t
    official_url "$url" || { echo "not an https official domain"; return; }
    text="$(fetch_text "$url")" || { echo "unreachable"; return; }
    [[ -n "$text" ]] || { echo "empty page"; return; }
    t="$(normalize <<<"$title" | sed 's/^ //; s/ $//')"
    [[ -z "$t" || " $text " == *" $t "* ]] || { echo "official title not found in page"; return; }
    [[ -z "$number" ]] || number_present "$text" "$number" || { echo "number $number not found in page"; return; }
    [[ -z "$year" || " $text " == *" $year "* ]] || { echo "year $year not found in page"; return; }
    echo ""
}

# Ementa from the Senado open-data legislation API (federal norms).
senado_ementa() {
    local tipo="$1" numero="$2" ano="$3" body
    body="$(curl -fsSL --retry 4 --retry-all-errors --retry-delay 5 -m 120 -A "$ua" -H 'Accept: application/json' \
        "$senado_api?tipo=$tipo&numero=$numero&ano=$ano")" || return 1
    jq -er --arg n "$numero" '
        [.ListaDocumento.documentos.documento | if type == "array" then .[] else . end
         | select(.numero == $n) | .ementa] | if length == 1 then .[0] else error("ambiguous") end' \
        <<<"$body"
}

validate() {
    local file="$1"
    [[ -f "$file" ]] || die "dataset not found: $file"
    jq -e --rawfile csv "$catalog" '
        def codes: [$csv | split("\n")[1:][] | select(length > 0) | split(",")
                    | select(.[1] == "national" or .[1] == "state") | .[0]];
        def states: [$csv | split("\n")[1:][] | select(length > 0) | split(",")
                     | select(.[1] == "state") | .[0]];
        .laws as $l
        | ($l | length > 0)
        and ([$l[].id] | length == (unique | length))
        and all($l[]; (.id | test("^[a-z0-9-]+$"))
                and ([.jurisdiction] | inside(codes))
                and (.url | test("^https://([a-z0-9-]+\\.)+(gov\\.br|leg\\.br|jus\\.br)(/|$)"))
                and (.title | length > 0) and (.summary | length > 0)
                and (.source | length > 0) and (.year | type == "number")
                and (.retrieved_on | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$")))
        and ([$l[] | select(.jurisdiction == "BR")] | length >= 10)
        and all(states[]; . as $s | [$l[] | select(.jurisdiction == $s)] | length | . >= 3 and . <= 5)
    ' "$file" >/dev/null || { echo "error: dataset failed validation: $file" >&2; return 1; }
    echo "valid: $file ($(jq '.laws | length' "$file") laws)"
}

build() {
    local date out
    date="$(date -u +%F)"
    out="$default_out"
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --date) date="$2"; shift 2 ;;
            --out) out="$2"; shift 2 ;;
            *) die "unknown argument: $1" ;;
        esac
    done
    [[ "$date" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || die "invalid --date: $date"
    local items failures=0 n i item id title reason
    items="$(mktemp)"
    n="$(jq 'length' "$curated")"
    : >"$items"
    for ((i = 0; i < n; i++)); do
        item="$(jq -c ".[$i]" "$curated")"
        id="$(jq -r '.id' <<<"$item")"
        title="$(jq -r '.title // ""' <<<"$item")"
        title_source="$(jq -r '.source' <<<"$item")"
        if jq -e '.senado' <<<"$item" >/dev/null; then
            # The norm must exist in the Senado API; its ementa is the title unless the
            # curated list pins the wording published at the official URL.
            if ! api_title="$(senado_ementa $(jq -r '.senado | "\(.tipo) \(.numero) \(.ano)"' <<<"$item"))"; then
                echo "FAIL $id: Senado API lookup failed" >&2
                failures=$((failures + 1))
                continue
            fi
            if [[ -z "$title" ]]; then
                title="$api_title"
                title_source="Senado Federal - API de dados abertos (legislacao)"
            fi
        fi
        [[ -n "$title" ]] || { echo "FAIL $id: no official title" >&2; failures=$((failures + 1)); continue; }
        reason="$(check_page "$(jq -r '.url' <<<"$item")" "$title" \
            "$(jq -r '.number // ""' <<<"$item")" "$(jq -r '.year' <<<"$item")")"
        if [[ -n "$reason" ]]; then
            echo "FAIL $id: $reason ($(jq -r '.url' <<<"$item"))" >&2
            failures=$((failures + 1))
            continue
        fi
        echo "ok   $id" >&2
        jq -c --arg t "$title" --arg ts "$title_source" --arg d "$date" \
            '{id, jurisdiction, kind, number: (.number // ""), year, slot, title: $t,
              title_source: $ts, summary, url, source, retrieved_on: $d}' <<<"$item" >>"$items"
    done
    if [[ $failures -gt 0 ]]; then
        rm -f "$items"
        die "$failures item(s) failed verification; dataset not written"
    fi
    mkdir -p "$(dirname "$out")"
    jq -s -S --indent 2 --slurpfile c <(jq '.' "$here/criterion.json") '
        {criterion: $c[0], generator: "tools/homolog-laws/build.sh",
         notice: "Metadados públicos de leis existentes para demonstração de homologação. Os resumos são do gerador, não textos oficiais.",
         laws: (sort_by((if .jurisdiction == "BR" then "00" else .jurisdiction end), .id))}' \
        "$items" >"$out.tmp"
    rm -f "$items"
    validate "$out.tmp" || { rm -f "$out.tmp"; exit 1; }
    mv "$out.tmp" "$out"
    echo "written: $out"
}

cmd="${1:-}"
shift || true
case "$cmd" in
    build) build "$@" ;;
    validate) validate "${1:-$default_out}" ;;
    verify)
        [[ $# -ge 2 ]] || die "usage: build.sh verify URL TITLE [NUMBER] [YEAR]"
        reason="$(check_page "$1" "$2" "${3:-}" "${4:-}")"
        [[ -z "$reason" ]] && echo "OK" || { echo "FAIL: $reason"; exit 1; }
        ;;
    *) die "usage: build.sh build|validate|verify (see header)" ;;
esac

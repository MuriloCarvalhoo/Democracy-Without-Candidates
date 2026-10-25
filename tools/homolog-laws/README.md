# homolog-laws

Rebuilds `catalog/br/homologacao/leis.json`: public metadata of existing high-impact laws
(Brazil and each of the 27 federative units) used by `dwc seed-homolog` to create
demonstrative proposals for homologation. The seed never needs the network; only this
generator does.

## Inputs

- `curated.json`: the curated list (id, jurisdiction code from `catalog/br/jurisdictions.csv`,
  kind, number, year, slot, official URL, source, generated plain-language summary). State
  items carry the official title (ementa) copied verbatim from the official page; federal
  items take it from the Senado open-data API (`senado` lookup) unless `title` pins the
  wording published at the URL.
- `criterion.json`: the inclusion criterion (objective classes F1–F5 national, E1–E5 state).
  It is an agent proposal for review.

## Usage

    tools/homolog-laws/build.sh build [--date YYYY-MM-DD] [--out FILE]   # or: just homolog-laws
    tools/homolog-laws/build.sh validate [FILE]                          # offline schema check
    tools/homolog-laws/build.sh verify URL TITLE [NUMBER] [YEAR]         # curation helper

Requires bash, curl, jq, xmllint, iconv and pdftotext.

`build` fetches every URL (HTTPS on `.gov.br`, `.leg.br` or `.jus.br` only, certificate
checked), requires the official title, number and year in the page text, validates the
result (catalog codes, unique ids, ≥10 national laws, 3–5 per state) and writes the dataset
sorted with stable formatting. If any source is unreachable or any item fails, it exits 1
and writes nothing: items are never dropped silently. To drop or replace a law, edit
`curated.json` and rebuild. Some state portals are slow or intermittently unreachable
(observed 2026-10-25: `elegis.al.ap.leg.br`, `ww2.al.rs.gov.br` DNS, `www3.sefaz.es.gov.br`);
curl retries each URL, and if a host stays down, rerun later or replace its URL. A full run
takes about 10 minutes.

Then seed a non-production database: `just seed-homolog <db>` (defaults to `data/dev.db`).

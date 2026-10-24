//! Versioned local territorial catalog (Brazil: IBGE codes). Imported idempotently at
//! startup; a changed file is applied as upsert so existing references stay valid.

use super::{
    clock::ts,
    crypto::{new_id, sha256_hex},
    db::{Db, query_opt},
};

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("database: {0}")]
    Db(#[from] turso::Error),
    #[error("catalog file {0}: {1}")]
    File(String, String),
}

pub async fn import(db: &Db, path: &str) -> Result<bool, CatalogError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| CatalogError::File(path.into(), e.to_string()))?;
    import_str(db, path, &content).await
}

pub async fn import_str(db: &Db, name: &str, content: &str) -> Result<bool, CatalogError> {
    let checksum = sha256_hex(content);
    let conn = db.read().await?;
    if let Some(row) = query_opt(
        &conn,
        "SELECT checksum FROM catalog_versions WHERE catalog = ?1",
        [name],
    )
    .await?
    {
        let current: String = row.get(0)?;
        if current == checksum {
            return Ok(false);
        }
    }

    let mut lines = content.lines();
    if lines.next() != Some("code,kind,name,parent_code,state_abbrev") {
        return Err(CatalogError::File(name.into(), "unexpected header".into()));
    }
    let tx = db.write().await?;
    for (n, line) in lines.enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let fields = parse_csv_line(line);
        let [code, kind, label, parent, abbrev] = fields.as_slice() else {
            return Err(CatalogError::File(
                name.into(),
                format!("line {}: expected 5 fields", n + 2),
            ));
        };
        let parent_id: Option<String> = if parent.is_empty() {
            None
        } else {
            let row = query_opt(
                &tx,
                "SELECT id FROM jurisdictions WHERE code = ?1",
                [parent.as_str()],
            )
            .await?
            .ok_or_else(|| {
                CatalogError::File(
                    name.into(),
                    format!("line {}: unknown parent {parent}", n + 2),
                )
            })?;
            Some(row.get(0)?)
        };
        let abbrev = (!abbrev.is_empty()).then_some(abbrev.as_str());
        tx.execute(
            "INSERT INTO jurisdictions (id, code, kind, name, parent_id, state_abbrev) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(code) DO UPDATE SET name = excluded.name, kind = excluded.kind,
               parent_id = excluded.parent_id, state_abbrev = excluded.state_abbrev",
            (new_id(), code.as_str(), kind.as_str(), label.as_str(), parent_id, abbrev),
        )
        .await?;
    }
    tx.execute(
        "INSERT INTO catalog_versions (catalog, checksum, imported_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(catalog) DO UPDATE SET checksum = excluded.checksum, imported_at = excluded.imported_at",
        (name, checksum, ts(chrono::Utc::now())),
    )
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// Minimal CSV field splitter supporting double-quoted fields.
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => fields.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    fields.push(current);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::db::testing::migrated;

    #[test]
    fn csv_quoted_fields() {
        assert_eq!(parse_csv_line(r#"1,"a, b",c"#), vec!["1", "a, b", "c"]);
    }

    #[tokio::test]
    async fn imports_brazil_catalog_once() {
        let t = migrated().await;
        assert!(import(&t.db, "catalog/br/jurisdictions.csv").await.unwrap());
        assert!(!import(&t.db, "catalog/br/jurisdictions.csv").await.unwrap());
        let conn = t.db.read().await.unwrap();
        let row = query_opt(
            &conn,
            "SELECT m.name, s.state_abbrev, n.code FROM jurisdictions m
             JOIN jurisdictions s ON s.id = m.parent_id JOIN jurisdictions n ON n.id = s.parent_id
             WHERE m.code = '4202404'",
            (),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(row.get::<String>(0).unwrap(), "Blumenau");
        assert_eq!(row.get::<String>(1).unwrap(), "SC");
        assert_eq!(row.get::<String>(2).unwrap(), "BR");
    }
}

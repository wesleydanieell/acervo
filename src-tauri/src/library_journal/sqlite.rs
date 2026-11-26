//! Transactional logical snapshot of the legacy catalog's reference columns.
//! Never copy SQLite/WAL bytes or replace the database file.
use super::{Image, Images, Resource, Step};
use crate::library_reference_impact::PathRemap;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MAX_ROWS: usize = 50_000;
const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Row {
    id: i64,
    title: Option<String>,
    author: Option<String>,
    paths: Option<String>,
    destination: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Snapshot {
    version: u8,
    schema: String,
    rows: Vec<Row>,
}

fn database(root: &Path) -> Result<Option<PathBuf>, String> {
    let Some(service) = crate::library_editions::service(root)? else {
        return Ok(None);
    };
    let db = service.join("Catalogo_Conhecimento.db");
    match fs::symlink_metadata(&db) {
        Ok(m) if m.file_type().is_file() => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        _ => return Err("Banco de catálogo inválido ou simbólico.".into()),
    }
    for name in [
        "Catalogo_Conhecimento.db-wal",
        "Catalogo_Conhecimento.db-shm",
        "Catalogo_Conhecimento.db-journal",
    ] {
        match fs::symlink_metadata(service.join(name)) {
            Ok(m) if m.file_type().is_file() => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err("Arquivo auxiliar SQLite inválido.".into()),
        }
    }
    Ok(Some(db))
}

fn open(db: &Path, write: bool) -> Result<Connection, String> {
    let flags = if write {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let c = Connection::open_with_flags(db, flags | OpenFlags::SQLITE_OPEN_NOFOLLOW)
        .map_err(|e| e.to_string())?;
    c.busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|e| e.to_string())?;
    if write {
        c.pragma_update(None, "foreign_keys", true)
            .map_err(|e| e.to_string())?;
    }
    Ok(c)
}

fn bounded_text(
    row: &rusqlite::Row<'_>,
    col: usize,
    remaining: &mut usize,
) -> Result<Option<String>, String> {
    use rusqlite::types::ValueRef;
    match row.get_ref(col).map_err(|e| e.to_string())? {
        ValueRef::Null => Ok(None),
        ValueRef::Text(bytes) => {
            *remaining = remaining
                .checked_sub(bytes.len())
                .ok_or("Snapshot SQLite excede limite.")?;
            Ok(Some(
                std::str::from_utf8(bytes)
                    .map_err(|_| "Texto SQLite não UTF-8.")?
                    .to_owned(),
            ))
        }
        _ => Err("Tipo de coluna de catálogo incompatível.".into()),
    }
}

fn read(c: &Connection) -> Result<Snapshot, String> {
    let mut remaining = MAX_BYTES;
    let mut schema_rows = Vec::new();
    {
        let mut q = c
            .prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")
            .map_err(|e| e.to_string())?;
        let mut rows = q.query([]).map_err(|e| e.to_string())?;
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            if schema_rows.len() >= 4096 {
                return Err("Schema SQLite excede limite.".into());
            }
            let ty = bounded_text(r, 0, &mut remaining)?.unwrap_or_default();
            let name = bounded_text(r, 1, &mut remaining)?.unwrap_or_default();
            let table = bounded_text(r, 2, &mut remaining)?.unwrap_or_default();
            let sql = bounded_text(r, 3, &mut remaining)?;
            if ty == "trigger" && table.eq_ignore_ascii_case("livros") {
                return Err("Triggers de catálogo exigem adaptador específico.".into());
            }
            schema_rows.push((ty, name, table, sql));
        }
    }
    // Do not permit cascades or other foreign-key side effects on this table.
    for (ty, name, _, _) in &schema_rows {
        if ty == "table" && !name.starts_with("sqlite_") {
            let found:i64=c.query_row("SELECT count(*) FROM pragma_foreign_key_list(?1) WHERE lower(\"table\")='livros'",[name],|r|r.get(0)).map_err(|e|e.to_string())?;
            if found != 0 {
                return Err("Dependência de chave estrangeira do catálogo não suportada.".into());
            }
        }
    }
    let schema = crate::library_reference_impact::revision(&schema_rows)?;
    let mut q = c
        .prepare(
            "SELECT id,titulo,autor,arquivos_encontrados,local_indicado FROM livros ORDER BY id",
        )
        .map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    let mut rows = q.query([]).map_err(|e| e.to_string())?;
    while let Some(r) = rows.next().map_err(|e| e.to_string())? {
        if result.len() >= MAX_ROWS {
            return Err("Catálogo excede limite de linhas.".into());
        }
        let id = r.get(0).map_err(|e| e.to_string())?;
        if result.last().is_some_and(|last: &Row| last.id >= id) {
            return Err("Identidade de catálogo duplicada.".into());
        }
        result.push(Row {
            id,
            title: bounded_text(r, 1, &mut remaining)?,
            author: bounded_text(r, 2, &mut remaining)?,
            paths: bounded_text(r, 3, &mut remaining)?,
            destination: bounded_text(r, 4, &mut remaining)?,
        });
    }
    Ok(Snapshot {
        version: 1,
        schema,
        rows: result,
    })
}

fn encode(s: &Snapshot) -> Result<Vec<u8>, String> {
    // Bound serialization before allocation can grow beyond the image limit.
    struct Bounded(Vec<u8>);
    impl std::io::Write for Bounded {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if b.len() > MAX_BYTES - self.0.len() {
                return Err(std::io::Error::other("Snapshot SQLite excede limite."));
            }
            self.0.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut out = Bounded(Vec::new());
    serde_json::to_writer(&mut out, s).map_err(|e| e.to_string())?;
    Ok(out.0)
}

fn decode(bytes: &[u8]) -> Result<Snapshot, String> {
    if bytes.len() > MAX_BYTES {
        return Err("Snapshot SQLite excede limite.".into());
    }
    let s: Snapshot = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if s.version != 1
        || !super::digest_valid(&s.schema)
        || s.rows.len() > MAX_ROWS
        || s.rows.windows(2).any(|w| w[0].id >= w[1].id)
        || encode(&s)? != bytes
    {
        return Err("Snapshot SQLite inválido ou não canônico.".into());
    }
    Ok(s)
}

fn capture(root: &Path) -> Result<Option<Snapshot>, String> {
    let Some(db) = database(root)? else {
        return Ok(None);
    };
    let mut c = open(&db, false)?;
    let tx = c.transaction().map_err(|e| e.to_string())?;
    let s = read(&tx)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Some(s))
}

pub(super) fn capture_bytes(root: &Path) -> Result<Option<Vec<u8>>, String> {
    capture(root)?.as_ref().map(encode).transpose()
}

#[allow(dead_code)]
pub(crate) fn plan(
    root: &Path,
    remap: &PathRemap<'_>,
    images: &mut Images,
) -> Result<Option<Step>, String> {
    let Some(before) = capture(root)? else {
        return Ok(None);
    };
    let mut after = before.clone();
    for row in &mut after.rows {
        if let Some(raw) = &row.paths {
            if !raw.trim().is_empty() && raw.trim() != "null" {
                let mut paths: Vec<String> =
                    serde_json::from_str(raw).map_err(|e| e.to_string())?;
                let mut changed = false;
                let mut targets = std::collections::HashMap::new();
                for path in &mut paths {
                    let old = path.clone();
                    if let Some(change) = remap.map(path)? {
                        *path = change.after;
                        changed = true;
                    }
                    if targets
                        .insert(path.clone(), old.clone())
                        .is_some_and(|previous| previous != old)
                    {
                        return Err("Colisão de associação SQLite.".into());
                    }
                }
                if changed {
                    row.paths = Some(serde_json::to_string(&paths).map_err(|e| e.to_string())?);
                }
            }
        }
        if let Some(destination) = &row.destination {
            if let Some(change) = remap.map(destination)? {
                row.destination = Some(change.after);
            }
        }
    }
    if before == after {
        return Ok(None);
    }
    Ok(Some(Step::RewriteReferences {
        resource: Resource::LegacyCatalog,
        before: images.insert(Some(encode(&before)?))?,
        after: images.insert(Some(encode(&after)?))?,
    }))
}

/// Internal adapter only. No command invokes it. Exact compare in an IMMEDIATE
/// transaction, with postcondition verification before commit.
#[allow(dead_code)]
pub(super) fn replace(root: &Path, expected: &[u8], desired: &[u8]) -> Result<(), String> {
    let before = decode(expected)?;
    let after = decode(desired)?;
    if before.schema != after.schema
        || before.rows.len() != after.rows.len()
        || before
            .rows
            .iter()
            .zip(&after.rows)
            .any(|(a, b)| a.id != b.id || a.title != b.title || a.author != b.author)
    {
        return Err("Snapshot não pode alterar identidades ou schema.".into());
    }
    let db = database(root)?.ok_or("Banco de catálogo ausente.")?;
    let mut c = open(&db, true)?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let actual = read(&tx)?;
    if actual == after {
        return Ok(());
    } // idempotent, no writes
    if actual != before {
        return Err("Catálogo divergiu do snapshot esperado.".into());
    }
    for (a, b) in before.rows.iter().zip(&after.rows) {
        if a.paths == b.paths && a.destination == b.destination {
            continue;
        }
        let changed=tx.execute("UPDATE livros SET arquivos_encontrados=?1,local_indicado=?2 WHERE id=?3 AND arquivos_encontrados IS ?4 AND local_indicado IS ?5",(&b.paths,&b.destination,a.id,&a.paths,&a.destination)).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("Referência SQLite divergiu durante atualização.".into());
        }
    }
    if read(&tx)? != after {
        return Err("Pós-condição SQLite divergente.".into());
    }
    tx.commit().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::tests::Fixture;
    use super::*;
    fn fixture(root: &Path) -> Connection {
        let service = root.join("99 - Catálogo");
        fs::create_dir(&service).unwrap();
        let c = Connection::open(service.join("Catalogo_Conhecimento.db")).unwrap();
        c.execute_batch("CREATE TABLE livros(id INTEGER PRIMARY KEY,titulo TEXT,autor TEXT,arquivos_encontrados TEXT,local_indicado TEXT,extra TEXT); INSERT INTO livros VALUES(1,'Book','Author','[ \"A/book.pdf\" ]','A/Future','untouched'),(2,'Absent',NULL,NULL,'A/Absent','extra'),(3,'Other',NULL,'null','AB','keep');").unwrap();
        c
    }
    #[test]
    fn exact_reference_images_roundtrip_with_wal_and_unrelated_values() {
        let f = Fixture::new();
        let c = fixture(&f.0);
        c.pragma_update(None, "journal_mode", "WAL").unwrap();
        c.execute("UPDATE livros SET extra='wal' WHERE id=1", [])
            .unwrap();
        let before = capture_bytes(&f.0).unwrap().unwrap();
        let mut images = Images::default();
        let step = plan(&f.0, &PathRemap::new(vec![("A", "New")]), &mut images)
            .unwrap()
            .unwrap();
        let mut after = decode(&before).unwrap();
        after.rows[0].paths = Some("[\"New/book.pdf\"]".into());
        after.rows[0].destination = Some("New/Future".into());
        after.rows[1].destination = Some("New/Absent".into());
        let after = encode(&after).unwrap();
        let Step::RewriteReferences {
            before: a,
            after: b,
            ..
        } = step
        else {
            panic!()
        };
        assert_eq!(a, Image::of(Some(&before)).unwrap());
        assert_eq!(b, Image::of(Some(&after)).unwrap());
        replace(&f.0, &before, &after).unwrap();
        replace(&f.0, &before, &after).unwrap();
        assert_eq!(capture_bytes(&f.0).unwrap().unwrap(), after);
        replace(&f.0, &after, &before).unwrap();
        assert_eq!(capture_bytes(&f.0).unwrap().unwrap(), before);
        assert_eq!(
            c.query_row("SELECT extra FROM livros WHERE id=1", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "wal"
        );
    }
    #[cfg(unix)]
    #[test]
    fn journal_inspection_recognizes_sqlite_before_and_after_without_rewriting() {
        use super::super::{Event, append, prepare, recovery};
        use crate::library_mutations::{RecoveryReservation, Reservation};
        let f = Fixture::new();
        let _c = fixture(&f.0);
        let guard = Reservation::acquire(&f.0).unwrap();
        let before = capture_bytes(&f.0).unwrap().unwrap();
        let mut after = decode(&before).unwrap();
        after.rows[0].destination = Some("New".into());
        let after = encode(&after).unwrap();
        let mut images = Images::default();
        let mut intent = f.intent();
        intent.steps = vec![Step::RewriteReferences {
            resource: Resource::LegacyCatalog,
            before: images.insert(Some(before.clone())).unwrap(),
            after: images.insert(Some(after.clone())).unwrap(),
        }];
        prepare(&guard, intent, &images).unwrap();
        append(&guard, Event::BeginStep { index: 0 }).unwrap();
        drop(guard);
        let recovery = RecoveryReservation::acquire(&f.0).unwrap();
        let initial = serde_json::to_value(recovery::inspect(&recovery).unwrap()).unwrap();
        assert_eq!(initial["evidence"], "before");
        replace(&f.0, &before, &after).unwrap();
        let result = serde_json::to_value(recovery::inspect(&recovery).unwrap()).unwrap();
        assert_eq!(result["evidence"], "after");
        assert_eq!(result["state"], initial["state"]);
    }

    #[test]
    fn rejects_triggers_and_foreign_key_dependents() {
        let f = Fixture::new();
        let c = fixture(&f.0);
        c.execute_batch("CREATE TRIGGER side_effect AFTER UPDATE ON livros BEGIN UPDATE livros SET extra='changed'; END;").unwrap();
        assert!(capture_bytes(&f.0).is_err());
        c.execute_batch("DROP TRIGGER side_effect; CREATE TABLE dependent (work INTEGER REFERENCES livros(id) ON UPDATE CASCADE);").unwrap();
        assert!(capture_bytes(&f.0).is_err());
    }

    #[test]
    fn conflict_and_constraint_failures_leave_all_rows_unchanged() {
        let f = Fixture::new();
        let c = fixture(&f.0);
        let before = capture_bytes(&f.0).unwrap().unwrap();
        let mut after = decode(&before).unwrap();
        after.rows[0].destination = Some("New".into());
        after.rows[1].destination = Some("New".into());
        let after = encode(&after).unwrap();
        c.execute("UPDATE livros SET local_indicado='External' WHERE id=2", [])
            .unwrap();
        assert!(replace(&f.0, &before, &after).is_err());
        assert_eq!(
            c.query_row("SELECT local_indicado FROM livros WHERE id=1", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "A/Future"
        );
        c.execute("UPDATE livros SET local_indicado='A/Absent' WHERE id=2", [])
            .unwrap();
        c.execute(
            "CREATE UNIQUE INDEX unique_dest ON livros(local_indicado)",
            [],
        )
        .unwrap();
        let before = capture_bytes(&f.0).unwrap().unwrap();
        let mut after = decode(&before).unwrap();
        after.rows[0].destination = Some("New".into());
        after.rows[1].destination = Some("New".into());
        assert!(replace(&f.0, &before, &encode(&after).unwrap()).is_err());
        assert_eq!(capture_bytes(&f.0).unwrap().unwrap(), before);
    }
}

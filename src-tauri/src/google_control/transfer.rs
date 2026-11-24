use super::*;
use serde::{Deserialize, Serialize};

const HEADER: &[&str] = &[
    "id",
    "email",
    "name",
    "color",
    "subscription",
    "activationDate",
    "expirationDate",
    "enabled",
    "archived",
    "favorite",
    "group",
    "tags",
    "notes",
];

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataFile {
    version: u32,
    accounts: Vec<GoogleAccountInput>,
}

pub fn export_metadata(conn: &Connection, format: &str) -> Result<String> {
    let accounts = snapshot(conn)?
        .accounts
        .into_iter()
        .map(|a| a.metadata)
        .collect::<Vec<_>>();
    match format {
        "json" => serde_json::to_string_pretty(&MetadataFile {
            version: 1,
            accounts,
        })
        .map_err(err),
        "csv" => {
            let mut rows = vec![HEADER.iter().map(|v| v.to_string()).collect::<Vec<_>>()];
            for a in accounts {
                rows.push(vec![
                    a.id.unwrap_or_default(),
                    a.email,
                    a.name,
                    a.color,
                    a.subscription,
                    a.activation_date.unwrap_or_default(),
                    a.expiration_date.unwrap_or_default(),
                    a.enabled.to_string(),
                    a.archived.to_string(),
                    a.favorite.to_string(),
                    a.group,
                    serde_json::to_string(&a.tags).map_err(err)?,
                    a.notes,
                ]);
            }
            Ok(rows
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|value| format!("\"{}\"", value.replace('"', "\"\"")))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .collect::<Vec<_>>()
                .join("\r\n")
                + "\r\n")
        }
        _ => Err("Metadata format must be json or csv".into()),
    }
}

pub fn import_metadata(conn: &Connection, data: &str, format: &str) -> Result<()> {
    if data.len() > 16 * 1024 * 1024 {
        return Err("Metadata import exceeds 16 MB".into());
    }
    let data = data.trim_start_matches('\u{feff}');
    let mut accounts = match format {
        "json" => {
            let file: MetadataFile = serde_json::from_str(data).map_err(err)?;
            if file.version != 1 {
                return Err("Unsupported metadata version".into());
            }
            file.accounts
        }
        "csv" => {
            let mut rows = parse_csv(data)?.into_iter();
            let header = rows.next().ok_or("CSV is empty")?;
            if header.iter().map(String::as_str).collect::<Vec<_>>() != HEADER {
                return Err("CSV header does not match the account metadata format".into());
            }
            let mut accounts = vec![];
            for (index, row) in rows.enumerate() {
                if row.len() != HEADER.len() {
                    return Err(format!(
                        "CSV row {} must contain {} columns",
                        index + 2,
                        HEADER.len()
                    ));
                }
                let optional = |n: usize| {
                    if row[n].is_empty() {
                        None
                    } else {
                        Some(row[n].clone())
                    }
                };
                let boolean = |n: usize| {
                    row[n]
                        .parse::<bool>()
                        .map_err(|_| format!("CSV row {} has an invalid boolean", index + 2))
                };
                accounts.push(GoogleAccountInput {
                    id: optional(0),
                    email: row[1].clone(),
                    name: row[2].clone(),
                    color: row[3].clone(),
                    subscription: row[4].clone(),
                    activation_date: optional(5),
                    expiration_date: optional(6),
                    enabled: boolean(7)?,
                    archived: boolean(8)?,
                    favorite: boolean(9)?,
                    group: row[10].clone(),
                    tags: serde_json::from_str(&row[11]).map_err(err)?,
                    notes: row[12].clone(),
                });
            }
            accounts
        }
        _ => return Err("Metadata format must be json or csv".into()),
    };
    if accounts.len() > 10_000 {
        return Err("At most 10,000 accounts can be imported at once".into());
    }
    let mut emails = HashSet::new();
    let mut ids = HashSet::new();
    for account in &mut accounts {
        validate_account(account)?;
        if !emails.insert(account.email.clone()) {
            return Err("Import contains duplicate email addresses".into());
        }
        if let Some(id) = &account.id {
            if !ids.insert(id.clone()) {
                return Err("Import contains duplicate account ids".into());
            }
        }
    }
    let tx = conn.unchecked_transaction().map_err(err)?;
    let count = accounts.len();
    for mut account in accounts {
        // Email is the import identity; foreign IDs must not overwrite unrelated local accounts.
        account.id = tx
            .query_row(
                "SELECT id FROM google_accounts WHERE email=?1 COLLATE NOCASE",
                [&account.email],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        upsert_account(&tx, account, false)?;
    }
    activity(
        &tx,
        "metadata_imported",
        None,
        None,
        "Account metadata imported",
        &format!("{count} accounts"),
    )?;
    tx.commit().map_err(err)
}

fn parse_csv(data: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut chars = data.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    let mut started = false;
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(ch);
            }
            continue;
        }
        match ch {
            '"' if !started && field.is_empty() && !closed => {
                quoted = true;
                started = true;
            }
            '"' => return Err("Unexpected quote in CSV field".into()),
            ',' => {
                row.push(std::mem::take(&mut field));
                closed = false;
                started = false;
            }
            '\r' | '\n' => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                closed = false;
                started = false;
            }
            _ if closed => return Err("Unexpected character after quoted CSV field".into()),
            _ => {
                field.push(ch);
                started = true;
            }
        }
    }
    if quoted {
        return Err("Unterminated quoted CSV field".into());
    }
    if started || closed || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

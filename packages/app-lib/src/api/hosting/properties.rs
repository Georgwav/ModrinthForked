//! Reading and changing `server.properties`, keeping its comments and the
//! order of its lines.

use super::server_dir;
use crate::util::io;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

const FILE: &str = "server.properties";

/// The server's properties, in file order. A server that never started has
/// only the ones set when it was built.
pub async fn server_properties(
    id: &str,
) -> crate::Result<Vec<(String, String)>> {
    let path = server_dir(id).await?.join(FILE);
    let text = if path.is_file() {
        tokio::fs::read_to_string(&path).await.map_err(|error| {
            crate::util::io::IOError::with_path(error, &path)
        })?
    } else {
        String::new()
    };
    Ok(parse(&text))
}

/// Sets properties, adding the ones the file doesn't have yet.
pub async fn set_server_properties(
    id: &str,
    values: HashMap<String, String>,
) -> crate::Result<()> {
    let dir = server_dir(id).await?;
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort();
    write_properties(&dir, &values).await?;
    // The port also lives in the server's settings.
    if let Some((_, port)) = values.iter().find(|(key, _)| key == "server-port")
        && let Ok(port) = port.trim().parse::<u16>()
    {
        super::edit_server(
            id,
            super::EditServer {
                port: Some(port),
                ..Default::default()
            },
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn set_property(
    dir: &Path,
    key: &str,
    value: &str,
) -> crate::Result<()> {
    write_properties(dir, &[(key.to_string(), value.to_string())]).await
}

/// Writes `values` into the directory's `server.properties`.
pub(super) async fn write_properties(
    dir: &Path,
    values: &[(String, String)],
) -> crate::Result<()> {
    let path = dir.join(FILE);
    let text = if path.is_file() {
        tokio::fs::read_to_string(&path).await.map_err(|error| {
            crate::util::io::IOError::with_path(error, &path)
        })?
    } else {
        String::new()
    };
    io::write(&path, update(&text, values)).await?;
    Ok(())
}

fn parse(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            if line.is_empty() || line.starts_with('#') || line.starts_with('!')
            {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((unescape(key.trim()), unescape(value)))
        })
        .collect()
}

fn update(text: &str, values: &[(String, String)]) -> String {
    let mut remaining = values.iter().collect::<Vec<_>>();
    let mut lines = text
        .lines()
        .map(|line| {
            let key = line
                .trim_start()
                .split_once('=')
                .filter(|_| !line.trim_start().starts_with('#'))
                .map(|(key, _)| unescape(key.trim()));
            if let Some(key) = key
                && let Some(index) =
                    remaining.iter().position(|(name, _)| *name == key)
            {
                let (name, value) = remaining.remove(index);
                format!("{}={}", escape(name), escape(value))
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>();
    lines.extend(
        remaining
            .into_iter()
            .map(|(name, value)| format!("{}={}", escape(name), escape(value))),
    );
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// Minecraft writes non-ASCII characters (like in a MOTD) as `\uXXXX` and
/// escapes `:`, `=` and backslashes.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ':' => out.push_str("\\:"),
            '=' => out.push_str("\\="),
            '\n' => out.push_str("\\n"),
            c if c.is_ascii() => out.push(c),
            c => {
                let mut units = [0_u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out
}

fn unescape(value: &str) -> String {
    let mut units = Vec::new();
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('u') => {
                    let hex = chars.by_ref().take(4).collect::<String>();
                    if let Ok(unit) = u16::from_str_radix(&hex, 16) {
                        units.push(unit);
                        continue;
                    }
                }
                Some('n') => push_char(&mut out, &mut units, '\n'),
                Some(other) => push_char(&mut out, &mut units, other),
                None => {}
            }
        } else {
            push_char(&mut out, &mut units, c);
        }
    }
    flush_units(&mut out, &mut units);
    out
}

fn push_char(out: &mut String, units: &mut Vec<u16>, c: char) {
    flush_units(out, units);
    out.push(c);
}

fn flush_units(out: &mut String, units: &mut Vec<u16>) {
    if !units.is_empty() {
        out.push_str(&String::from_utf16_lossy(units));
        units.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_values_and_keeps_comments() {
        let text =
            "#Minecraft server properties\nmotd=Old\nserver-port=25565\n";
        let updated = update(
            text,
            &[
                ("server-port".to_string(), "25570".to_string()),
                ("level-seed".to_string(), "42".to_string()),
            ],
        );
        assert_eq!(
            updated,
            "#Minecraft server properties\nmotd=Old\nserver-port=25570\nlevel-seed=42\n"
        );
        assert_eq!(
            parse(&updated),
            [
                ("motd".to_string(), "Old".to_string()),
                ("server-port".to_string(), "25570".to_string()),
                ("level-seed".to_string(), "42".to_string()),
            ]
        );
    }

    #[test]
    fn round_trips_special_characters() {
        let motd = "Héllo: a=b \\ ✓ 🎉";
        let text = update("", &[("motd".to_string(), motd.to_string())]);
        assert!(text.is_ascii(), "{text}");
        assert_eq!(parse(&text), [("motd".to_string(), motd.to_string())]);
    }
}

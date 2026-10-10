//! Minimal protobuf reader/writer for Discord's user settings protos
//! (server folders & order, favourite GIFs). Unknown fields are kept verbatim
//! so a modified proto can be sent back without losing anything.

use base64::Engine;

/// One top-level field: number, wire type and raw payload.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Field<'a> {
    pub num: u32,
    pub wire: u8,
    /// Varint value for wire type 0, otherwise unused.
    pub int: u64,
    /// Bytes for wire types 1, 2 and 5 (fixed64 / length-delimited / fixed32).
    pub data: &'a [u8],
    /// The complete encoded field (tag + payload), for verbatim copies.
    pub raw: &'a [u8],
}

fn varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut out = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *buf.get(*pos)?;
        *pos += 1;
        out |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Some(out);
        }
    }
    None
}

pub fn fields(buf: &[u8]) -> Vec<Field<'_>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let start = pos;
        let Some(tag) = varint(buf, &mut pos) else {
            break;
        };
        let (num, wire) = ((tag >> 3) as u32, (tag & 7) as u8);
        let (int, data) = match wire {
            0 => match varint(buf, &mut pos) {
                Some(v) => (v, &buf[0..0]),
                None => break,
            },
            1 => {
                let Some(d) = buf.get(pos..pos + 8) else {
                    break;
                };
                pos += 8;
                (0, d)
            }
            2 => {
                let Some(len) = varint(buf, &mut pos) else {
                    break;
                };
                let Some(d) = buf.get(pos..pos + len as usize) else {
                    break;
                };
                pos += len as usize;
                (0, d)
            }
            5 => {
                let Some(d) = buf.get(pos..pos + 4) else {
                    break;
                };
                pos += 4;
                (0, d)
            }
            _ => break,
        };
        out.push(Field {
            num,
            wire,
            int,
            data,
            raw: &buf[start..pos],
        });
    }
    out
}

fn first<'a>(buf: &'a [u8], num: u32) -> Option<Field<'a>> {
    fields(buf).into_iter().find(|f| f.num == num)
}

/// Value inside a google.protobuf.*Value wrapper (field 1).
fn wrapped_int(buf: &[u8]) -> Option<u64> {
    first(buf, 1).map(|f| f.int)
}

fn wrapped_str(buf: &[u8]) -> Option<String> {
    first(buf, 1).map(|f| String::from_utf8_lossy(f.data).into_owned())
}

/// Repeated fixed64 field, packed or not.
fn fixed64s(buf: &[u8], num: u32) -> Vec<u64> {
    let mut out = Vec::new();
    for f in fields(buf).into_iter().filter(|f| f.num == num) {
        let chunks: &[u8] = f.data;
        for c in chunks.chunks_exact(8) {
            out.push(u64::from_le_bytes(c.try_into().unwrap()));
        }
    }
    out
}

pub fn decode_b64(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

pub fn encode_b64(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}

// ---- server folders (PreloadedUserSettings.guild_folders = 14) ---------------

#[derive(Clone, Debug)]
pub struct Folder {
    /// `None` for the anonymous single-guild "folders".
    pub id: Option<i64>,
    pub name: Option<String>,
    pub color: Option<u32>,
    pub guild_ids: Vec<String>,
}

pub fn guild_folders(preloaded: &[u8]) -> Vec<Folder> {
    let Some(gf) = first(preloaded, 14) else {
        return vec![];
    };
    fields(gf.data)
        .into_iter()
        .filter(|f| f.num == 1)
        .map(|f| Folder {
            guild_ids: fixed64s(f.data, 1)
                .into_iter()
                .map(|g| g.to_string())
                .collect(),
            id: first(f.data, 2)
                .and_then(|w| wrapped_int(w.data))
                .map(|v| v as i64),
            name: first(f.data, 3)
                .and_then(|w| wrapped_str(w.data))
                .filter(|n| !n.is_empty()),
            color: first(f.data, 4)
                .and_then(|w| wrapped_int(w.data))
                .map(|v| v as u32),
        })
        .collect()
}

// ---- favourite GIFs (FrecencyUserSettings.favorite_gifs = 2) -----------------

#[derive(Clone, Debug, PartialEq)]
pub struct FavGif {
    /// Page / embed URL (the map key), what gets posted in chat.
    pub url: String,
    /// Media URL (GIF or MP4).
    pub src: String,
    pub width: u32,
    pub height: u32,
    pub order: u32,
}

pub fn favorite_gifs(frecency: &[u8]) -> Vec<FavGif> {
    let Some(fg) = first(frecency, 2) else {
        return vec![];
    };
    let mut out: Vec<FavGif> = fields(fg.data)
        .into_iter()
        .filter(|f| f.num == 1)
        .filter_map(|entry| {
            let key = first(entry.data, 1)?;
            let val = first(entry.data, 2)?;
            let v = fields(val.data);
            let get = |n: u32| v.iter().find(|f| f.num == n);
            Some(FavGif {
                url: String::from_utf8_lossy(key.data).into_owned(),
                src: get(2)
                    .map(|f| String::from_utf8_lossy(f.data).into_owned())
                    .unwrap_or_default(),
                width: get(3).map(|f| f.int as u32).unwrap_or(0),
                height: get(4).map(|f| f.int as u32).unwrap_or(0),
                order: get(5).map(|f| f.int as u32).unwrap_or(0),
            })
        })
        .collect();
    out.sort_by_key(|g| std::cmp::Reverse(g.order));
    out
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn put_tag(out: &mut Vec<u8>, num: u32, wire: u8) {
    put_varint(out, ((num as u64) << 3) | wire as u64);
}

fn put_bytes(out: &mut Vec<u8>, num: u32, data: &[u8]) {
    put_tag(out, num, 2);
    put_varint(out, data.len() as u64);
    out.extend_from_slice(data);
}

fn put_uint(out: &mut Vec<u8>, num: u32, v: u64) {
    put_tag(out, num, 0);
    put_varint(out, v);
}

/// Rewrites the frecency proto with a new favourite-GIF list, keeping every other field.
pub fn with_favorite_gifs(frecency: &[u8], gifs: &[FavGif]) -> Vec<u8> {
    let mut out = Vec::new();
    let old = first(frecency, 2);
    for f in fields(frecency) {
        if f.num != 2 {
            out.extend_from_slice(f.raw);
        }
    }
    let mut fg = Vec::new();
    for g in gifs {
        let mut val = Vec::new();
        put_uint(&mut val, 1, if g.src.ends_with(".mp4") { 2 } else { 1 });
        put_bytes(&mut val, 2, g.src.as_bytes());
        put_uint(&mut val, 3, g.width as u64);
        put_uint(&mut val, 4, g.height as u64);
        put_uint(&mut val, 5, g.order as u64);
        let mut entry = Vec::new();
        put_bytes(&mut entry, 1, g.url.as_bytes());
        put_bytes(&mut entry, 2, &val);
        put_bytes(&mut fg, 1, &entry);
    }
    // Keep hide_tooltip and anything else that lived next to the map.
    if let Some(old) = old {
        for f in fields(old.data) {
            if f.num != 1 {
                fg.extend_from_slice(f.raw);
            }
        }
    }
    put_bytes(&mut out, 2, &fg);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_roundtrip_shape() {
        // folder { guild_ids: [1,2] (packed), id: {1: 7}, name: {1: "Jeux"}, color: {1: 0xff0000} }
        let mut folder = Vec::new();
        let mut ids = Vec::new();
        ids.extend_from_slice(&1u64.to_le_bytes());
        ids.extend_from_slice(&2u64.to_le_bytes());
        put_bytes(&mut folder, 1, &ids);
        let mut w = Vec::new();
        put_uint(&mut w, 1, 7);
        put_bytes(&mut folder, 2, &w);
        let mut n = Vec::new();
        put_bytes(&mut n, 1, b"Jeux");
        put_bytes(&mut folder, 3, &n);
        let mut c = Vec::new();
        put_uint(&mut c, 1, 0xff0000);
        put_bytes(&mut folder, 4, &c);
        let mut gf = Vec::new();
        put_bytes(&mut gf, 1, &folder);
        let mut pre = Vec::new();
        put_uint(&mut pre, 99, 5); // unrelated field
        put_bytes(&mut pre, 14, &gf);
        let f = guild_folders(&pre);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].guild_ids, vec!["1", "2"]);
        assert_eq!(f[0].id, Some(7));
        assert_eq!(f[0].name.as_deref(), Some("Jeux"));
        assert_eq!(f[0].color, Some(0xff0000));
    }

    #[test]
    fn favorite_gifs_rewrite_keeps_other_fields() {
        let mut base = Vec::new();
        put_uint(&mut base, 3, 42);
        let gifs = vec![FavGif {
            url: "https://tenor.com/view/x".into(),
            src: "https://media.tenor.com/x.gif".into(),
            width: 200,
            height: 100,
            order: 3,
        }];
        let out = with_favorite_gifs(&base, &gifs);
        assert_eq!(favorite_gifs(&out), gifs);
        assert!(fields(&out).iter().any(|f| f.num == 3 && f.int == 42));
    }
}

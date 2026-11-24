use std::path::PathBuf;

const FILES: &[(&str, &str)] = &[
    ("base64", include_str!("../../../stdlib/base64.zsharp")),
    ("csv", include_str!("../../../stdlib/csv.zsharp")),
    ("hash", include_str!("../../../stdlib/hash.zsharp")),
    ("random", include_str!("../../../stdlib/random.zsharp")),
    ("text", include_str!("../../../stdlib/text.zsharp")),
    ("json", include_str!("../../../stdlib/json.zsharp")),
    ("net", include_str!("../../../stdlib/net.zsharp")),
    ("os", include_str!("../../../stdlib/os.zsharp")),
    ("path", include_str!("../../../stdlib/path.zsharp")),
    ("regex", include_str!("../../../stdlib/regex.zsharp")),
    ("time", include_str!("../../../stdlib/time.zsharp")),
    ("toml", include_str!("../../../stdlib/toml.zsharp")),
];

fn fnv(data: &[u8], mut h: u64) -> u64 { for &b in data { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); } h }

fn cache_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    let mut h = 0xcbf29ce484222325u64;
    for (n, c) in FILES { h = fnv(c.as_bytes(), fnv(n.as_bytes(), h)); }
    let dir = base.join("z-sharp").join(format!("std-{}-{:016x}", env!("CARGO_PKG_VERSION"), h));
    if FILES.iter().all(|(n, c)| std::fs::read_to_string(dir.join(format!("{n}.zsharp"))).map_or(false, |t| t == *c)) { return Some(dir); }
    std::fs::create_dir_all(&dir).ok()?;
    for (n, c) in FILES {
        let tmp = dir.join(format!(".{n}.{}.tmp", std::process::id()));
        std::fs::write(&tmp, c).ok()?;
        std::fs::rename(&tmp, dir.join(format!("{n}.zsharp"))).ok()?;
    }
    Some(dir)
}

/// `Std.<Nazwa>` -> plik źródłowy modułu.
pub fn locate(name: &str) -> Option<PathBuf> {
    let stem = name.to_ascii_lowercase();
    let mut dirs: Vec<PathBuf> = vec![];
    if let Some(d) = std::env::var_os("ZS_STD") { dirs.push(PathBuf::from(d)); }
    if let Some(h) = std::env::var_os("ZS_HOME") { dirs.push(PathBuf::from(h).join("stdlib")); }
    for d in dirs {
        if let Some(f) = zs_manifest::with_source_ext(&d.join(&stem)) { return Some(f); }
    }
    if !FILES.iter().any(|(n, _)| *n == stem) { return None; }
    zs_manifest::with_source_ext(&cache_dir()?.join(&stem))
}

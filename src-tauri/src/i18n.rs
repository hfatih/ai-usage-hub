pub fn text(key: &str, language: &str) -> &'static str {
    match (language, key) {
        ("tr", "open") => "Genel bakışı aç",
        ("tr", "refresh") => "Durumu yenile",
        ("tr", "pause") => "İzlemeyi duraklat / sürdür",
        ("tr", "settings") => "Ayarlar",
        ("tr", "codex") => "Codex'i aç",
        ("tr", "claude") => "Claude'u aç",
        ("tr", "gemini") => "Gemini'yi aç",
        ("tr", "antigravity") => "Antigravity'yi aç",
        ("tr", "quit") => "Çıkış",
        ("tr", "tooltip") => "Yerel yapay zekâ uygulamaları izleniyor",
        (_, "open") => "Open Dashboard",
        (_, "refresh") => "Refresh Status",
        (_, "pause") => "Pause / Resume Monitoring",
        (_, "settings") => "Settings",
        (_, "codex") => "Open Codex",
        (_, "claude") => "Open Claude",
        (_, "gemini") => "Open Gemini",
        (_, "antigravity") => "Open Antigravity",
        (_, "quit") => "Quit",
        _ => "Monitoring local AI applications",
    }
}

pub fn running_tooltip(count: usize, language: &str) -> String {
    if language == "tr" {
        format!("AI Usage Hub\n{count} uygulama çalışıyor")
    } else {
        format!("AI Usage Hub\n{count} app(s) running")
    }
}

/// Wording for a desktop toast built from a stored (English) usage alert.
pub fn alert_text(title: &str, detail: Option<&str>, language: &str) -> (String, String) {
    let detail = detail.unwrap_or_default();
    if language != "tr" {
        return (title.to_owned(), detail.to_owned());
    }
    let title = title.replace(" usage alert", " kullanım uyarısı").replace(" refresh failed", " yenilenemedi");
    let detail = if let Some((label, rest)) = detail.split_once(" reset to ") {
        format!("{label} sıfırlandı; kalan %{}", rest.trim_end_matches("% remaining"))
    } else if let Some((label, rest)) = detail.split_once(" has ") {
        format!("{label} için kalan %{}", rest.trim_end_matches("% remaining"))
    } else {
        detail.to_owned()
    };
    (title, detail)
}

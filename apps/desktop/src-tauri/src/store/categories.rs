//! Shared keyword taxonomy for catalog pagination and installed Store filters.
use serde::Serialize;

#[derive(Serialize)]
pub struct Category { pub id: &'static str, pub label: &'static str, pub keywords: &'static [&'static str] }

/// A lightweight classification from names/descriptions, not an upstream certification.
#[tauri::command]
pub fn store_categories() -> &'static [Category] {
    &[
        Category { id: "frontend", label: "Frontend & Design", keywords: &["frontend", "front-end", "react", "angular", "svelte", "vue", "css", "tailwind", "ui", "ux", "figma", "design", "accessibility"] },
        Category { id: "backend", label: "Backend & APIs", keywords: &["backend", "back-end", "api", "apis", "server", "graphql", "rest", "django", "fastapi", "express", "spring", "authentication"] },
        Category { id: "security", label: "Security", keywords: &["security", "audit", "vulnerability", "vulnerabilities", "pentest", "owasp", "threat", "secrets", "encryption"] },
        Category { id: "devops", label: "DevOps & Cloud", keywords: &["devops", "docker", "kubernetes", "terraform", "deploy", "deployment", "ci", "cd", "aws", "azure", "cloudflare", "cloud", "infrastructure", "monitoring"] },
        Category { id: "data", label: "Data & Databases", keywords: &["data", "database", "databases", "sql", "postgres", "postgresql", "mysql", "sqlite", "redis", "mongodb", "analytics", "spreadsheet", "excel"] },
        Category { id: "browser", label: "Browser & Testing", keywords: &["browser", "chrome", "devtools", "playwright", "puppeteer", "selenium", "testing", "test", "e2e"] },
        Category { id: "media", label: "Video, Image & Audio", keywords: &["video", "editor", "ffmpeg", "image", "images", "audio", "media", "animation", "render", "editing", "photoshop", "premiere"] },
        Category { id: "productivity", label: "Documents & Productivity", keywords: &["document", "documents", "pdf", "word", "slides", "presentation", "email", "calendar", "notion", "slack", "notes", "tasks"] },
        Category { id: "other", label: "Other", keywords: &[] },
    ]
}

/// Match whole tokens to avoid classifying `build` as UI or `server` as REST.
pub fn matches(text: &str, category: &str) -> bool {
    if category.is_empty() || category == "all" { return true; }
    let text = text.to_lowercase();
    let words: Vec<_> = text.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    let categories = store_categories();
    let matching: Vec<_> = categories.iter().filter(|c| c.keywords.iter().any(|key| {
        if key.contains('-') { text.contains(key) } else { words.contains(key) }
    })).collect();
    if category == "other" { matching.is_empty() } else { matching.iter().any(|c| c.id == category) }
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn domains_allow_multiple_matches_and_preserve_other_without_substring_noise() {
        assert!(matches("React security audit", "frontend"));
        assert!(matches("React security audit", "security"));
        assert!(matches("Chrome DevTools browser", "browser"));
        assert!(matches("FFmpeg video editing", "media"));
        assert!(matches("Build helper", "other"));
        assert!(!matches("Build helper", "frontend"));
        assert!(!matches("Restaurant reservations", "backend"));
        assert!(matches("Something unknown", "all"));
    }
}

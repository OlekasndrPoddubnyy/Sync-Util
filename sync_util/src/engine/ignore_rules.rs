//! Regole di esclusione con la sintassi di .gitignore, valutate su percorsi relativi alla cartella radice
//! (separatore "/"). Il confronto ignora maiuscole/minuscole, come il file system di Windows.
//!
//! - righe vuote e righe che iniziano con # sono ignorate (\# per un nome che inizia con #)
//! - "nome" vale a qualsiasi livello; se la regola contiene una "/" iniziale o intermedia è relativa alla radice
//! - una "/" finale limita la regola alle cartelle
//! - caratteri jolly: * (qualsiasi testo tranne /), ? (un carattere), [a-z], ** (qualsiasi livello di cartelle)
//! - "!" in testa re-include quanto escluso da una regola precedente: vince l'ultima regola che corrisponde
//!
//! Per comodità su Windows anche "\" è accettato come separatore, se non precede un carattere speciale.

use regex::Regex;

/// Regole proposte al primo avvio: le esclusioni storiche dello strumento.
pub const DEFAULT_TEXT: &str = "\
# Una regola per riga, come in .gitignore. Le righe con # sono commenti.

# Controllo versione e IDE
.git
.gitignore
.github
.vscode
.idea
.settings
.classpath
.project
.apt_generated

# Output di build
target
*.ear

# Pipeline, analisi e documentazione
starter-pipeline-java.yaml
starter-pipeline-auto-fix.yaml
starter-pipeline-unit-test.yaml
sonar-analysis.bat
sonar-project.properties
readme.md
TEST_SUITE_README.md
COVERAGE_REPORT.md
copilot-instructions.md
report.html
";

const ESCAPABLE: &str = "*?[]\\!# ";

#[derive(Debug, Clone)]
struct Rule {
    pattern: Regex,
    negated: bool,
    directory_only: bool,
    anchored: bool,
}

/// Riga del testo (numerata da 1) che non è stato possibile interpretare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidLine {
    pub number: usize,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct IgnoreRules {
    rules: Vec<Rule>,
    invalid_lines: Vec<InvalidLine>,
}

impl IgnoreRules {
    pub fn parse(text: &str) -> Self {
        let mut rules = Vec::new();
        let mut invalid_lines = Vec::new();
        for (index, raw) in split_lines(text).into_iter().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match parse_rule(line) {
                Ok(Some(rule)) => rules.push(rule),
                Ok(None) => {}
                Err(_) => invalid_lines.push(InvalidLine { number: index + 1, text: line.to_string() }),
            }
        }
        IgnoreRules { rules, invalid_lines }
    }

    /// `relative_path`: percorso relativo alla radice, con "/" come separatore (es. "modulo/target").
    pub fn is_ignored(&self, relative_path: &str, directory: bool) -> bool {
        let name = relative_path.rsplit('/').next().unwrap_or(relative_path);
        let mut ignored = false;
        for rule in &self.rules {
            if rule.negated == ignored
                && (!rule.directory_only || directory)
                && rule.pattern.is_match(if rule.anchored { relative_path } else { name })
            {
                ignored = !rule.negated;
            }
        }
        ignored
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn invalid_lines(&self) -> &[InvalidLine] {
        &self.invalid_lines
    }
}

/// Divide come `\R` in Java: \r\n, \n, \r e i separatori di riga Unicode.
fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let is_break = matches!(c, '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}');
        if is_break {
            lines.push(&text[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r'
                && let Some(&(j, '\n')) = chars.peek() {
                    chars.next();
                    end = j + 1;
                }
            start = end;
        }
    }
    lines.push(&text[start..]);
    lines
}

fn parse_rule(line: &str) -> Result<Option<Rule>, regex::Error> {
    let negated = line.starts_with('!');
    let mut glob = normalize_separators(if negated { &line[1..] } else { line });
    let directory_only = glob.ends_with('/');
    while glob.ends_with('/') {
        glob.pop();
    }
    let anchored = glob.contains('/');
    if let Some(stripped) = glob.strip_prefix('/') {
        glob = stripped.to_string();
    }
    if glob.is_empty() {
        return Ok(None);
    }
    let pattern = Regex::new(&format!("(?i)^(?:{})$", glob_to_regex(&glob)))?;
    Ok(Some(Rule { pattern, negated, directory_only, anchored }))
}

/// "\" seguito da un carattere speciale è un escape, altrimenti è un separatore di cartella (abitudine di Windows).
fn normalize_separators(glob: &str) -> String {
    let chars: Vec<char> = glob.chars().collect();
    let mut out = String::with_capacity(glob.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '\\' {
            out.push(c);
        } else if i + 1 < chars.len() && ESCAPABLE.contains(chars[i + 1]) {
            out.push(c);
            i += 1;
            out.push(chars[i]);
        } else {
            out.push('/');
        }
        i += 1;
    }
    out
}

fn glob_to_regex(glob: &str) -> String {
    let chars: Vec<char> = glob.chars().collect();
    let n = chars.len();
    let mut regex = String::new();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c == '*' {
            let mut end = i;
            while end < n && chars[end] == '*' {
                end += 1;
            }
            let whole_segment =
                end - i >= 2 && (i == 0 || chars[i - 1] == '/') && (end == n || chars[end] == '/');
            if !whole_segment {
                regex.push_str("[^/]*");
            } else if end == n {
                regex.push_str(".*"); // "dir/**": tutto il contenuto
            } else {
                regex.push_str("(?:.*/)?"); // "**/": zero o più cartelle
                end += 1;
            }
            i = end;
            continue;
        }
        let class_end = if c == '[' { class_end(&chars, i) } else { None };
        if let Some(close) = class_end {
            regex.push_str(&char_class(&chars[i + 1..close]));
            i = close;
        } else if c == '?' {
            regex.push_str("[^/]");
        } else if c == '\\' && i + 1 < n {
            i += 1;
            regex.push_str(&regex::escape(&chars[i].to_string()));
        } else {
            regex.push_str(&regex::escape(&c.to_string()));
        }
        i += 1;
    }
    regex
}

/// Indice della "]" che chiude la classe aperta in `start`, se c'è (altrimenti la "[" è un carattere normale).
fn class_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start + 1;
    if i < chars.len() && (chars[i] == '!' || chars[i] == '^') {
        i += 1;
    }
    if i < chars.len() && chars[i] == ']' {
        i += 1; // "]" subito dopo l'apertura fa parte della classe
    }
    chars[i.min(chars.len())..].iter().position(|&c| c == ']').map(|p| p + i)
}

fn char_class(content: &[char]) -> String {
    let mut out = String::from("[");
    let mut i = 0;
    if matches!(content.first(), Some('!') | Some('^')) {
        out.push('^');
        i = 1;
    }
    for &c in &content[i..] {
        // "-" resta un intervallo; gli altri caratteri con un significato nelle classi vanno protetti
        if matches!(c, '\\' | '[' | ']' | '&' | '^' | '~') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push_str("&&[^/]]");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(text: &str) -> IgnoreRules {
        IgnoreRules::parse(text)
    }

    #[test]
    fn name_matches_at_any_level_case_insensitive() {
        let r = rules("target\n*.ear");
        assert!(r.is_ignored("target", true));
        assert!(r.is_ignored("mod/Target", true));
        assert!(r.is_ignored("a/b/app.EAR", false));
        assert!(!r.is_ignored("targets", true));
    }

    #[test]
    fn anchored_and_directory_only() {
        let r = rules("/build\ndocs/\nsrc/gen");
        assert!(r.is_ignored("build", true));
        assert!(!r.is_ignored("x/build", true));
        assert!(r.is_ignored("docs", true));
        assert!(!r.is_ignored("docs", false));
        assert!(r.is_ignored("src/gen", true));
        assert!(!r.is_ignored("a/src/gen", true));
    }

    #[test]
    fn double_star_and_negation() {
        let r = rules("**/temp\nlogs/**\n*.log\n!keep.log");
        assert!(r.is_ignored("temp", true));
        assert!(r.is_ignored("a/b/temp", true));
        assert!(r.is_ignored("logs/x/y.txt", false));
        assert!(r.is_ignored("a.log", false));
        assert!(!r.is_ignored("keep.log", false));
    }

    #[test]
    fn classes_question_mark_and_backslash() {
        let r = rules("file?.[ch]\nsrc\\tmp\n\\#hash");
        assert!(r.is_ignored("file1.c", false));
        assert!(!r.is_ignored("file12.c", false));
        assert!(r.is_ignored("src/tmp", true));
        assert!(r.is_ignored("#hash", false));
    }

    #[test]
    fn invalid_lines_are_reported() {
        let r = rules("ok\n[z-a]\r\nalso_ok");
        assert_eq!(r.len(), 2);
        assert_eq!(r.invalid_lines(), &[InvalidLine { number: 2, text: "[z-a]".into() }]);
    }

    #[test]
    fn defaults_parse_cleanly() {
        let r = rules(DEFAULT_TEXT);
        assert!(r.invalid_lines().is_empty());
        assert!(r.is_ignored(".git", true));
        assert!(r.is_ignored("README.md", false));
    }
}

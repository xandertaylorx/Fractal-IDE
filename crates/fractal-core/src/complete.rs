//! Local "smart" completion: identifiers from your code, language keywords and
//! snippets, ranked by prefix match, proximity and frequency. No network.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Word,
    Keyword,
    Snippet,
    /// Came from a language server; see `lsp_kind`.
    Lsp,
}

#[derive(Clone, Debug)]
pub struct Suggestion {
    pub label: String,
    /// Text that replaces the typed prefix. May contain `$0` (final cursor) and `\n`.
    pub insert: String,
    pub kind: Kind,
    pub detail: String,
    /// LSP CompletionItemKind (only for `Kind::Lsp`).
    pub lsp_kind: u32,
    pub deprecated: bool,
    /// Absolute char index where the replacement starts (defaults to the typed prefix).
    pub edit_start: Option<usize>,
}

fn kw(list: &str) -> Vec<&str> {
    list.split_whitespace().collect()
}

fn keywords(lang: &str) -> Vec<&'static str> {
    let s: &'static str = match lang {
        "rust" => "as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while Option Result Some None Ok Err String Vec Box println! format! vec! assert! assert_eq! panic! todo! unimplemented! derive Debug Clone Copy PartialEq Default",
        "python" => "and as assert async await break class continue def del elif else except False finally for from global if import in is lambda None nonlocal not or pass raise return True try while with yield print len range enumerate zip isinstance self str int float list dict set tuple open super __init__ __name__ __main__",
        "javascript" | "javascriptreact" => "async await break case catch class const continue debugger default delete do else export extends false finally for function if import in instanceof let new null of return static super switch this throw true try typeof undefined var void while with yield console document window require module exports Promise Array Object JSON Math setTimeout fetch",
        "typescript" | "typescriptreact" => "abstract any as async await boolean break case catch class const constructor continue declare default do else enum export extends false finally for from function if implements import in instanceof interface keyof let namespace never new null number of private protected public readonly return static string super switch this throw true try type typeof undefined unknown var void while yield console document window Promise Array Record Partial Readonly",
        "java" => "abstract assert boolean break byte case catch char class const continue default do double else enum extends final finally float for if implements import instanceof int interface long native new null package private protected public return short static super switch synchronized this throw throws transient true try void volatile while String System ArrayList HashMap List Map Set Optional Override println",
        "kotlin" => "abstract as break by class companion const continue data do else enum false for fun if import in interface internal is lateinit null object open override package private protected public return sealed super this throw true try typealias val var when while println",
        "c" | "cpp" => "auto break case char class const constexpr continue default delete do double else enum explicit extern false float for friend goto if inline int long namespace new nullptr operator private protected public register return short signed sizeof static struct switch template this throw true try typedef typename union unsigned using virtual void volatile while include define printf scanf malloc free std string vector cout cin endl size_t",
        "csharp" => "abstract as async await base bool break byte case catch char class const continue decimal default delegate do double else enum event false finally float for foreach if in int interface internal is lock long namespace new null object out override params private protected public readonly ref return sealed short static string struct switch this throw true try typeof using var virtual void while Console WriteLine List Dictionary",
        "go" => "break case chan const continue default defer else fallthrough for func go goto if import interface map package range return select struct switch type var bool byte error int int64 string true false nil make new len cap append copy panic recover fmt Println Printf",
        "php" => "abstract and array as break case catch class clone const continue declare default do echo else elseif empty extends false final finally for foreach function global if implements include interface isset list namespace new null or private protected public require return static switch this throw true try use var while",
        "ruby" => "alias and begin break case class def defined do else elsif end ensure false for if in module next nil not or puts redo rescue retry return self super then true undef unless until when while yield attr_accessor require",
        "swift" => "as break case catch class continue default defer do else enum extension false for func guard if import in init let nil private public return self struct super switch throw true try var while print String Int Double Bool Array Dictionary",
        "html" => "div span p a img ul ol li table tr td th thead tbody form input button label select option textarea h1 h2 h3 h4 h5 h6 header footer nav main section article aside script style link meta head body html title class id href src alt type value placeholder name rel stylesheet charset viewport",
        "css" | "scss" | "less" => "display position flex grid margin padding border background color font-size font-family font-weight line-height width height min-width max-width min-height max-height overflow z-index top right bottom left align-items justify-content flex-direction gap transition transform opacity cursor box-shadow border-radius text-align content none block inline inline-block absolute relative fixed sticky center auto inherit important",
        "shellscript" => "if then else elif fi for while do done case esac function return exit echo export local read cd ls grep sed awk cat mkdir rm cp mv chmod source set unset",
        "sql" => "SELECT FROM WHERE INSERT INTO VALUES UPDATE SET DELETE CREATE TABLE ALTER DROP INDEX JOIN LEFT RIGHT INNER OUTER ON GROUP BY ORDER HAVING LIMIT OFFSET DISTINCT AS AND OR NOT NULL IN LIKE BETWEEN COUNT SUM AVG MIN MAX PRIMARY KEY FOREIGN REFERENCES",
        "lua" => "and break do else elseif end false for function goto if in local nil not or repeat return then true until while print require table string math",
        _ => "",
    };
    kw(s)
}

struct Snip(&'static str, &'static str, &'static str);

fn snippets(lang: &str) -> Vec<Snip> {
    match lang {
        "rust" => vec![
            Snip("fn", "fn ${name}($0) {\n    \n}", "function"),
            Snip("main", "fn main() {\n    $0\n}", "main function"),
            Snip("impl", "impl ${Type} {\n    $0\n}", "impl block"),
            Snip("struct", "struct ${Name} {\n    $0\n}", "struct"),
            Snip("enum", "enum ${Name} {\n    $0\n}", "enum"),
            Snip("for", "for ${item} in ${iter} {\n    $0\n}", "for loop"),
            Snip("if", "if ${cond} {\n    $0\n}", "if"),
            Snip("match", "match ${expr} {\n    $0\n}", "match"),
            Snip("test", "#[test]\nfn ${name}() {\n    $0\n}", "test"),
            Snip("pr", "println!(\"{}\", $0);", "println!"),
        ],
        "python" => vec![
            Snip("def", "def ${name}($0):\n    pass", "function"),
            Snip("class", "class ${Name}:\n    def __init__(self$0):\n        pass", "class"),
            Snip("for", "for ${item} in ${items}:\n    $0", "for loop"),
            Snip("if", "if ${cond}:\n    $0", "if"),
            Snip("ifmain", "if __name__ == \"__main__\":\n    $0", "main guard"),
            Snip("try", "try:\n    $0\nexcept Exception as e:\n    raise", "try/except"),
            Snip("with", "with open(${path}) as f:\n    $0", "with open"),
            Snip("pr", "print($0)", "print"),
        ],
        "javascript" | "typescript" | "javascriptreact" | "typescriptreact" => vec![
            Snip("fn", "function ${name}($0) {\n  \n}", "function"),
            Snip("af", "const ${name} = ($0) => {\n  \n};", "arrow function"),
            Snip("for", "for (let ${i} = 0; ${i} < ${n}; ${i}++) {\n  $0\n}", "for loop"),
            Snip("forof", "for (const ${item} of ${items}) {\n  $0\n}", "for...of"),
            Snip("if", "if (${cond}) {\n  $0\n}", "if"),
            Snip("log", "console.log($0);", "console.log"),
            Snip("try", "try {\n  $0\n} catch (e) {\n  console.error(e);\n}", "try/catch"),
            Snip("imp", "import ${name} from '$0';", "import"),
            Snip("class", "class ${Name} {\n  constructor($0) {\n  }\n}", "class"),
        ],
        "java" => vec![
            Snip("main", "public static void main(String[] args) {\n    $0\n}", "main method"),
            Snip("psvm", "public static void main(String[] args) {\n    $0\n}", "main method"),
            Snip("sout", "System.out.println($0);", "System.out.println"),
            Snip("for", "for (int ${i} = 0; ${i} < ${n}; ${i}++) {\n    $0\n}", "for loop"),
            Snip("fore", "for (${Type} ${item} : ${items}) {\n    $0\n}", "for-each"),
            Snip("if", "if (${cond}) {\n    $0\n}", "if"),
            Snip("class", "public class ${Name} {\n    $0\n}", "class"),
            Snip("try", "try {\n    $0\n} catch (Exception e) {\n    e.printStackTrace();\n}", "try/catch"),
        ],
        "c" | "cpp" => vec![
            Snip("main", "int main(int argc, char** argv) {\n    $0\n    return 0;\n}", "main"),
            Snip("for", "for (int ${i} = 0; ${i} < ${n}; ${i}++) {\n    $0\n}", "for loop"),
            Snip("if", "if (${cond}) {\n    $0\n}", "if"),
            Snip("inc", "#include <$0>", "#include"),
        ],
        "go" => vec![
            Snip("main", "func main() {\n\t$0\n}", "main"),
            Snip("fn", "func ${name}($0) {\n\t\n}", "function"),
            Snip("for", "for ${i} := 0; ${i} < ${n}; ${i}++ {\n\t$0\n}", "for loop"),
            Snip("iferr", "if err != nil {\n\treturn err\n}", "if err"),
            Snip("pr", "fmt.Println($0)", "fmt.Println"),
        ],
        "html" => vec![
            Snip("html5", "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <title>${Document}</title>\n</head>\n<body>\n  $0\n</body>\n</html>", "HTML5 boilerplate"),
            Snip("!", "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <title>${Document}</title>\n</head>\n<body>\n  $0\n</body>\n</html>", "HTML5 boilerplate"),
            Snip("link", "<link rel=\"stylesheet\" href=\"$0\">", "stylesheet link"),
            Snip("script", "<script src=\"$0\"></script>", "script tag"),
        ],
        "shellscript" => vec![Snip("if", "if [ ${cond} ]; then\n    $0\nfi", "if"), Snip("for", "for ${i} in ${items}; do\n    $0\ndone", "for loop")],
        _ => vec![],
    }
}

/// Strip `${placeholder}` markers down to their default text, leaving `$0` alone.
pub fn expand_placeholders(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next();
            let mut name = String::new();
            for n in chars.by_ref() {
                if n == '}' {
                    break;
                }
                name.push(n);
            }
            out.push_str(&name);
        } else {
            out.push(c);
        }
    }
    out
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Count identifiers in `text` (length >= 3).
pub fn index_words(text: &str, into: &mut HashMap<String, u32>) {
    let mut cur = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if is_ident(c) {
            cur.push(c);
        } else {
            if cur.chars().count() >= 3 && !cur.chars().next().unwrap().is_ascii_digit() && cur.len() < 48 {
                *into.entry(std::mem::take(&mut cur)).or_insert(0) += 1;
            }
            cur.clear();
        }
    }
}

fn subsequence(hay: &str, needle: &str) -> bool {
    let mut it = hay.chars().map(|c| c.to_ascii_lowercase());
    needle.chars().map(|c| c.to_ascii_lowercase()).all(|n| it.any(|h| h == n))
}

pub struct Inputs<'a> {
    pub prefix: &'a str,
    pub lang: &'a str,
    /// Words near the caret in the current file (counted).
    pub local_words: &'a HashMap<String, u32>,
    /// Words from other open files.
    pub other_words: &'a HashMap<String, u32>,
    pub max: usize,
}

/// Ranked suggestions for the identifier prefix before the caret.
pub fn suggest(inp: &Inputs) -> Vec<Suggestion> {
    let prefix = inp.prefix;
    if prefix.is_empty() {
        return vec![];
    }
    let lp = prefix.to_ascii_lowercase();
    let mut scored: Vec<(i64, Suggestion)> = vec![];
    let mut seen = std::collections::HashSet::new();
    let score = |label: &str, base: i64| -> Option<i64> {
        let ll = label.to_ascii_lowercase();
        if ll == lp && label == prefix {
            return None; // already fully typed
        }
        if label.starts_with(prefix) {
            Some(base + 300 - label.len() as i64)
        } else if ll.starts_with(&lp) {
            Some(base + 260 - label.len() as i64)
        } else if prefix.chars().count() >= 2 && subsequence(label, prefix) {
            // camelCase / snake_case initials beat arbitrary subsequences
            let initials: String = label.split('_').filter(|p| !p.is_empty()).filter_map(|p| p.chars().next()).collect::<String>().to_ascii_lowercase();
            let camel: String = label.char_indices().filter(|(i, c)| *i == 0 || c.is_uppercase()).map(|(_, c)| c.to_ascii_lowercase()).collect();
            if initials.starts_with(&lp) || camel.starts_with(&lp) {
                Some(base + 150 - label.len() as i64)
            } else {
                Some(base + 40 - label.len() as i64)
            }
        } else {
            None
        }
    };
    for s in snippets(inp.lang) {
        if s.0.starts_with(prefix) && s.0 != "" {
            let sc = if s.0 == prefix { 520 } else { 330 - s.0.len() as i64 };
            if seen.insert(format!("snip:{}", s.0)) {
                scored.push((sc, Suggestion { label: s.0.to_string(), insert: expand_placeholders(s.1), kind: Kind::Snippet, detail: s.2.to_string(), lsp_kind: 0, deprecated: false, edit_start: None }));
            }
        }
    }
    for k in keywords(inp.lang) {
        if let Some(sc) = score(k, 120) {
            if seen.insert(k.to_string()) {
                scored.push((sc, Suggestion { label: k.to_string(), insert: k.to_string(), kind: Kind::Keyword, detail: String::new(), lsp_kind: 0, deprecated: false, edit_start: None }));
            }
        }
    }
    for (maps, bonus) in [(inp.local_words, 200i64), (inp.other_words, 90i64)] {
        for (w, n) in maps.iter() {
            if let Some(sc) = score(w, bonus + (*n as i64).min(20)) {
                if seen.insert(w.clone()) {
                    scored.push((sc, Suggestion { label: w.clone(), insert: w.clone(), kind: Kind::Word, detail: String::new(), lsp_kind: 0, deprecated: false, edit_start: None }));
                }
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.label.cmp(&b.1.label)));
    scored.into_iter().take(inp.max).map(|(_, s)| s).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_words_rank_first_and_snippets_match() {
        let mut local = HashMap::new();
        index_words("let total_amount = compute_total(items); total_amount += 1;", &mut local);
        let other = HashMap::new();
        let r = suggest(&Inputs { prefix: "tot", lang: "rust", local_words: &local, other_words: &other, max: 8 });
        assert_eq!(r[0].label, "total_amount");
        let r = suggest(&Inputs { prefix: "ma", lang: "rust", local_words: &local, other_words: &other, max: 8 });
        assert!(r.iter().any(|s| s.label == "main" && s.kind == Kind::Snippet));
        // fully typed word is not suggested to itself
        let r = suggest(&Inputs { prefix: "total_amount", lang: "rust", local_words: &local, other_words: &other, max: 8 });
        assert!(r.iter().all(|s| s.label != "total_amount"));
    }

    #[test]
    fn camel_case_initials() {
        let mut local = HashMap::new();
        index_words("getUserProfileData()", &mut local);
        let r = suggest(&Inputs { prefix: "gupd", lang: "javascript", local_words: &local, other_words: &HashMap::new(), max: 8 });
        assert_eq!(r[0].label, "getUserProfileData");
    }
}

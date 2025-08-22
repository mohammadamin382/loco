use clap::Parser;
use colored::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(name = "line-counter")]
#[command(about = "A fast CLI tool to count lines of code in projects")]
struct Args {
    /// Path to the project directory
    #[arg(short, long)]
    path: PathBuf,
    
    /// Show detailed output
    #[arg(short, long)]
    verbose: bool,
    
    /// Output format (text/json)
    #[arg(short, long, default_value = "text")]
    format: String,
    
    /// Exclude directories (comma-separated)
    #[arg(short, long)]
    exclude: Option<String>,
    
    /// Include only specific file extensions (comma-separated)
    #[arg(short, long)]
    include: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LanguageStats {
    total_lines: usize,
    code_lines: usize,
    comment_lines: usize,
    blank_lines: usize,
    files: usize,
}

impl LanguageStats {
    fn new() -> Self {
        Self {
            total_lines: 0,
            code_lines: 0,
            comment_lines: 0,
            blank_lines: 0,
            files: 0,
        }
    }
    
    fn add(&mut self, other: &LanguageStats) {
        self.total_lines += other.total_lines;
        self.code_lines += other.code_lines;
        self.comment_lines += other.comment_lines;
        self.blank_lines += other.blank_lines;
        self.files += other.files;
    }
}

struct LanguageConfig {
    single_line_comments: Vec<String>,
    multi_line_comments: Vec<(String, String)>,
}

fn get_language_config(extension: &str) -> Option<LanguageConfig> {
    match extension {
        "rs" => Some(LanguageConfig {
            single_line_comments: vec!["//".to_string()],
            multi_line_comments: vec![("/*".to_string(), "*/".to_string())],
        }),
        "py" => Some(LanguageConfig {
            single_line_comments: vec!["#".to_string()],
            multi_line_comments: vec![("\"\"\"".to_string(), "\"\"\"".to_string()), ("'''".to_string(), "'''".to_string())],
        }),
        "js" | "ts" | "jsx" | "tsx" | "mjs" | "cjs" => Some(LanguageConfig {
            single_line_comments: vec!["//".to_string()],
            multi_line_comments: vec![("/*".to_string(), "*/".to_string())],
        }),
        "java" | "kt" | "scala" => Some(LanguageConfig {
            single_line_comments: vec!["//".to_string()],
            multi_line_comments: vec![("/*".to_string(), "*/".to_string())],
        }),
        "c" | "cpp" | "cc" | "cxx" | "c++" | "h" | "hpp" | "hxx" => Some(LanguageConfig {
            single_line_comments: vec!["//".to_string()],
            multi_line_comments: vec![("/*".to_string(), "*/".to_string())],
        }),
        "cs" => Some(LanguageConfig {
            single_line_comments: vec!["//".to_string()],
            multi_line_comments: vec![("/*".to_string(), "*/".to_string())],
        }),
        "go" => Some(LanguageConfig {
            single_line_comments: vec!["//".to_string()],
            multi_line_comments: vec![("/*".to_string(), "*/".to_string())],
        }),
        "rb" | "rake" => Some(LanguageConfig {
            single_line_comments: vec!["#".to_string()],
            multi_line_comments: vec![("=begin".to_string(), "=end".to_string())],
        }),
        "php" => Some(LanguageConfig {
            single_line_comments: vec!["//".to_string(), "#".to_string()],
            multi_line_comments: vec![("/*".to_string(), "*/".to_string())],
        }),
        "html" | "htm" | "xml" | "xhtml" | "svg" => Some(LanguageConfig {
            single_line_comments: vec![],
            multi_line_comments: vec![("<!--".to_string(), "-->".to_string())],
        }),
        "css" | "scss" | "sass" | "less" => Some(LanguageConfig {
            single_line_comments: vec!["//".to_string()],
            multi_line_comments: vec![("/*".to_string(), "*/".to_string())],
        }),
        "sh" | "bash" | "zsh" | "fish" => Some(LanguageConfig {
            single_line_comments: vec!["#".to_string()],
            multi_line_comments: vec![],
        }),
        "sql" | "mysql" | "pgsql" => Some(LanguageConfig {
            single_line_comments: vec!["--".to_string(), "#".to_string()],
            multi_line_comments: vec![("/*".to_string(), "*/".to_string())],
        }),
        "lua" => Some(LanguageConfig {
            single_line_comments: vec!["--".to_string()],
            multi_line_comments: vec![("--[[".to_string(), "]]".to_string())],
        }),
        "vim" => Some(LanguageConfig {
            single_line_comments: vec!["\"".to_string()],
            multi_line_comments: vec![],
        }),
        "r" => Some(LanguageConfig {
            single_line_comments: vec!["#".to_string()],
            multi_line_comments: vec![],
        }),
        "swift" => Some(LanguageConfig {
            single_line_comments: vec!["//".to_string()],
            multi_line_comments: vec![("/*".to_string(), "*/".to_string())],
        }),
        "dart" => Some(LanguageConfig {
            single_line_comments: vec!["//".to_string()],
            multi_line_comments: vec![("/*".to_string(), "*/".to_string())],
        }),
        "zig" => Some(LanguageConfig {
            single_line_comments: vec!["//".to_string()],
            multi_line_comments: vec![],
        }),
        "haskell" | "hs" => Some(LanguageConfig {
            single_line_comments: vec!["--".to_string()],
            multi_line_comments: vec![("{-".to_string(), "-}".to_string())],
        }),
        "elm" => Some(LanguageConfig {
            single_line_comments: vec!["--".to_string()],
            multi_line_comments: vec![("{-".to_string(), "-}".to_string())],
        }),
        "erlang" | "erl" => Some(LanguageConfig {
            single_line_comments: vec!["%".to_string()],
            multi_line_comments: vec![],
        }),
        "elixir" | "ex" | "exs" => Some(LanguageConfig {
            single_line_comments: vec!["#".to_string()],
            multi_line_comments: vec![],
        }),
        "clojure" | "clj" | "cljs" => Some(LanguageConfig {
            single_line_comments: vec![";".to_string()],
            multi_line_comments: vec![],
        }),
        "lisp" | "cl" => Some(LanguageConfig {
            single_line_comments: vec![";".to_string()],
            multi_line_comments: vec![],
        }),
        "scheme" | "scm" => Some(LanguageConfig {
            single_line_comments: vec![";".to_string()],
            multi_line_comments: vec![],
        }),
        "perl" | "pl" | "pm" => Some(LanguageConfig {
            single_line_comments: vec!["#".to_string()],
            multi_line_comments: vec![("=pod".to_string(), "=cut".to_string())],
        }),
        "powershell" | "ps1" => Some(LanguageConfig {
            single_line_comments: vec!["#".to_string()],
            multi_line_comments: vec![("<#".to_string(), "#>".to_string())],
        }),
        "dockerfile" => Some(LanguageConfig {
            single_line_comments: vec!["#".to_string()],
            multi_line_comments: vec![],
        }),
        "makefile" | "mk" => Some(LanguageConfig {
            single_line_comments: vec!["#".to_string()],
            multi_line_comments: vec![],
        }),
        "toml" => Some(LanguageConfig {
            single_line_comments: vec!["#".to_string()],
            multi_line_comments: vec![],
        }),
        "ini" | "cfg" | "conf" => Some(LanguageConfig {
            single_line_comments: vec![";".to_string(), "#".to_string()],
            multi_line_comments: vec![],
        }),
        _ => None,
    }
}

fn get_language_from_extension(extension: &str) -> String {
    match extension {
        "rs" => "Rust",
        "py" | "pyw" | "pyi" => "Python",
        "js" | "mjs" | "cjs" => "JavaScript",
        "ts" => "TypeScript",
        "jsx" => "JSX",
        "tsx" => "TSX",
        "java" => "Java",
        "kt" => "Kotlin",
        "scala" => "Scala",
        "c" => "C",
        "cpp" | "cc" | "cxx" | "c++" => "C++",
        "cs" => "C#",
        "h" => "C Header",
        "hpp" | "hxx" => "C++ Header",
        "go" => "Go",
        "rb" | "rake" => "Ruby",
        "php" => "PHP",
        "html" | "htm" | "xhtml" => "HTML",
        "css" => "CSS",
        "scss" => "SCSS",
        "sass" => "Sass",
        "less" => "Less",
        "xml" | "svg" => "XML",
        "sh" | "bash" | "zsh" | "fish" => "Shell",
        "sql" | "mysql" | "pgsql" => "SQL",
        "json" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "ini" | "cfg" | "conf" => "Config",
        "md" | "markdown" => "Markdown",
        "txt" | "text" => "Text",
        "lua" => "Lua",
        "vim" => "Vim Script",
        "r" => "R",
        "swift" => "Swift",
        "dart" => "Dart",
        "zig" => "Zig",
        "haskell" | "hs" => "Haskell",
        "elm" => "Elm",
        "erlang" | "erl" => "Erlang",
        "elixir" | "ex" | "exs" => "Elixir",
        "clojure" | "clj" | "cljs" => "Clojure",
        "lisp" | "cl" => "Lisp",
        "scheme" | "scm" => "Scheme",
        "perl" | "pl" | "pm" => "Perl",
        "powershell" | "ps1" => "PowerShell",
        "dockerfile" => "Dockerfile",
        "makefile" | "mk" => "Makefile",
        "gitignore" => "Gitignore",
        "license" => "License",
        "readme" => "Readme",
        _ => "Unknown",
    }.to_string()
}

fn analyze_file(file_path: &Path) -> Option<(String, LanguageStats)> {
    let extension = file_path.extension()?.to_str()?;
    let language = get_language_from_extension(extension);
    let config = get_language_config(extension);
    
    let content = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(_) => return None,
    };
    
    let mut stats = LanguageStats::new();
    stats.files = 1;
    
    let lines: Vec<&str> = content.lines().collect();
    stats.total_lines = lines.len();
    
    if let Some(config) = config {
        let mut in_multi_comment = false;
        let mut multi_comment_end = String::new();
        
        for line in lines {
            let trimmed = line.trim();
            
            if trimmed.is_empty() {
                stats.blank_lines += 1;
                continue;
            }
            
            let mut is_comment = false;
            let mut line_content = trimmed;
            
            // Check for multi-line comment continuation
            if in_multi_comment {
                is_comment = true;
                if let Some(end_pos) = line_content.find(&multi_comment_end) {
                    in_multi_comment = false;
                    line_content = &line_content[end_pos + multi_comment_end.len()..].trim();
                    if line_content.is_empty() {
                        stats.comment_lines += 1;
                        continue;
                    }
                } else {
                    stats.comment_lines += 1;
                    continue;
                }
            }
            
            // Check for multi-line comment start
            for (start, end) in &config.multi_line_comments {
                if let Some(start_pos) = line_content.find(start) {
                    let before_comment = &line_content[..start_pos].trim();
                    if !before_comment.is_empty() {
                        // Mixed line (code + comment)
                        stats.code_lines += 1;
                        break;
                    }
                    
                    if let Some(end_pos) = line_content[start_pos + start.len()..].find(end) {
                        // Single line multi-comment
                        let after_comment = &line_content[start_pos + start.len() + end_pos + end.len()..].trim();
                        if after_comment.is_empty() {
                            is_comment = true;
                        } else {
                            stats.code_lines += 1;
                        }
                    } else {
                        // Multi-line comment starts
                        in_multi_comment = true;
                        multi_comment_end = end.clone();
                        is_comment = true;
                    }
                    break;
                }
            }
            
            if !is_comment {
                // Check for single-line comments
                let mut found_single_comment = false;
                for comment_start in &config.single_line_comments {
                    if let Some(pos) = line_content.find(comment_start) {
                        let before_comment = &line_content[..pos].trim();
                        if before_comment.is_empty() {
                            is_comment = true;
                        } else {
                            // Mixed line (code + comment)
                            stats.code_lines += 1;
                        }
                        found_single_comment = true;
                        break;
                    }
                }
                
                if !found_single_comment && !is_comment {
                    stats.code_lines += 1;
                }
            }
            
            if is_comment {
                stats.comment_lines += 1;
            }
        }
    } else {
        // For unknown file types, count all non-blank lines as code
        stats.code_lines = stats.total_lines - stats.blank_lines;
        for line in lines {
            if line.trim().is_empty() {
                stats.blank_lines += 1;
            }
        }
    }
    
    Some((language, stats))
}

fn collect_files(path: &Path, exclude_dirs: &Option<String>, include_exts: &Option<String>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    
    // Parse exclude directories
    let exclude_set: std::collections::HashSet<String> = if let Some(exclude) = exclude_dirs {
        exclude.split(',').map(|s| s.trim().to_string()).collect()
    } else {
        std::collections::HashSet::new()
    };
    
    // Parse include extensions
    let include_set: Option<std::collections::HashSet<String>> = if let Some(include) = include_exts {
        Some(include.split(',').map(|s| s.trim().to_string()).collect())
    } else {
        None
    };
    
    // Default exclude directories
    let default_excludes = [
        "target", "node_modules", ".git", "build", "dist", "__pycache__", 
        ".cargo", ".next", ".nuxt", "vendor", "coverage", ".pytest_cache",
        ".vscode", ".idea", "bin", "obj", ".vs", "packages", ".svn", ".hg"
    ];
    
    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let file_path = entry.path();
        
        // Skip hidden files (but allow .gitignore, .env files, etc.)
        if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') && !matches!(name, ".gitignore" | ".env" | ".dockerignore" | ".replit") {
                continue;
            }
        }
        
        // Check against exclude directories
        let path_str = file_path.to_string_lossy();
        let mut should_exclude = false;
        
        // Check default excludes
        for exclude in &default_excludes {
            if path_str.contains(&format!("/{}/", exclude)) || path_str.contains(&format!("\\{}\\", exclude)) {
                should_exclude = true;
                break;
            }
        }
        
        // Check user-defined excludes
        for exclude in &exclude_set {
            if path_str.contains(&format!("/{}/", exclude)) || path_str.contains(&format!("\\{}\\", exclude)) {
                should_exclude = true;
                break;
            }
        }
        
        if should_exclude {
            continue;
        }
        
        // Check file extension if include filter is set
        if let Some(ref include_exts) = include_set {
            if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                if !include_exts.contains(ext) {
                    continue;
                }
            } else {
                // No extension, skip if include filter is active
                continue;
            }
        }
        
        files.push(file_path.to_path_buf());
    }
    
    files
}

fn print_results(stats: &HashMap<String, LanguageStats>, verbose: bool) {
    let mut total_stats = LanguageStats::new();
    
    println!("{}", "📊 Code Analysis Results".bright_cyan().bold());
    println!("{}", "=".repeat(60).bright_black());
    
    let mut sorted_languages: Vec<_> = stats.iter().collect();
    sorted_languages.sort_by(|a, b| b.1.total_lines.cmp(&a.1.total_lines));
    
    for (language, lang_stats) in &sorted_languages {
        total_stats.add(lang_stats);
        
        println!("\n{} {}", "🔹".bright_blue(), language.bright_white().bold());
        println!("  {} {} files", "📁".yellow(), lang_stats.files.to_string().bright_white());
        println!("  {} {} total lines", "📏".green(), lang_stats.total_lines.to_string().bright_white());
        println!("  {} {} code lines", "💻".bright_green(), lang_stats.code_lines.to_string().bright_white());
        println!("  {} {} comment lines", "💬".bright_blue(), lang_stats.comment_lines.to_string().bright_white());
        println!("  {} {} blank lines", "⬜".bright_black(), lang_stats.blank_lines.to_string().bright_white());
        
        if verbose {
            let code_percentage = if lang_stats.total_lines > 0 {
                (lang_stats.code_lines as f64 / lang_stats.total_lines as f64) * 100.0
            } else { 0.0 };
            println!("  {} {:.1}% code ratio", "📊".cyan(), code_percentage.to_string().bright_white());
        }
    }
    
    println!("\n{}", "📈 Total Summary".bright_magenta().bold());
    println!("{}", "=".repeat(60).bright_black());
    println!("  {} {} total files", "📁".yellow(), total_stats.files.to_string().bright_white());
    println!("  {} {} total lines", "📏".green(), total_stats.total_lines.to_string().bright_white());
    println!("  {} {} code lines", "💻".bright_green(), total_stats.code_lines.to_string().bright_white());
    println!("  {} {} comment lines", "💬".bright_blue(), total_stats.comment_lines.to_string().bright_white());
    println!("  {} {} blank lines", "⬜".bright_black(), total_stats.blank_lines.to_string().bright_white());
    
    let code_percentage = if total_stats.total_lines > 0 {
        (total_stats.code_lines as f64 / total_stats.total_lines as f64) * 100.0
    } else { 0.0 };
    println!("  {} {:.1}% overall code ratio", "📊".cyan(), code_percentage.to_string().bright_white());
}

fn print_json_results(stats: &HashMap<String, LanguageStats>) {
    let json_output = serde_json::json!({
        "languages": stats,
        "summary": {
            "total_files": stats.values().map(|s| s.files).sum::<usize>(),
            "total_lines": stats.values().map(|s| s.total_lines).sum::<usize>(),
            "total_code_lines": stats.values().map(|s| s.code_lines).sum::<usize>(),
            "total_comment_lines": stats.values().map(|s| s.comment_lines).sum::<usize>(),
            "total_blank_lines": stats.values().map(|s| s.blank_lines).sum::<usize>(),
        }
    });
    
    println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
}

fn main() {
    let args = Args::parse();
    
    if !args.path.exists() {
        eprintln!("{} Path does not exist: {}", "❌".bright_red(), args.path.display());
        std::process::exit(1);
    }
    
    if !args.path.is_dir() {
        eprintln!("{} Path is not a directory: {}", "❌".bright_red(), args.path.display());
        std::process::exit(1);
    }
    
    println!("{} Analyzing project: {}", "🔍".bright_yellow(), args.path.display().to_string().bright_white());
    
    let start_time = std::time::Instant::now();
    
    let files = collect_files(&args.path, &args.exclude, &args.include);
    println!("{} Found {} files to analyze", "📂".bright_blue(), files.len().to_string().bright_white());
    
    if files.is_empty() {
        println!("{} No files found matching the criteria.", "⚠️".bright_yellow());
        return;
    }
    
    let stats = Arc::new(Mutex::new(HashMap::<String, LanguageStats>::new()));
    let total_size = Arc::new(Mutex::new(0u64));
    
    // Process files in parallel for speed
    files.par_iter().for_each(|file_path| {
        if let Some((language, file_stats)) = analyze_file(file_path) {
            let mut stats_guard = stats.lock().unwrap();
            let entry = stats_guard.entry(language).or_insert_with(LanguageStats::new);
            entry.add(&file_stats);
            
            // Add file size
            if let Ok(metadata) = fs::metadata(file_path) {
                let mut size_guard = total_size.lock().unwrap();
                *size_guard += metadata.len();
            }
        }
    });
    
    let final_stats = Arc::try_unwrap(stats).unwrap().into_inner().unwrap();
    let final_size = Arc::try_unwrap(total_size).unwrap().into_inner().unwrap();
    let elapsed = start_time.elapsed();
    
    if final_stats.is_empty() {
        println!("{} No code files found in the specified directory.", "⚠️".bright_yellow());
        return;
    }
    
    match args.format.as_str() {
        "json" => print_json_results(&final_stats),
        _ => {
            print_results(&final_stats, args.verbose);
            println!("\n{} Performance", "⚡".bright_yellow().bold());
            println!("{}", "=".repeat(60).bright_black());
            println!("  {} {:.2} MB analyzed", "💾".cyan(), final_size as f64 / 1_048_576.0);
            println!("  {} {:.2} seconds", "⏱️".green(), elapsed.as_secs_f64());
            println!("  {} {:.0} files/second", "🚀".bright_green(), files.len() as f64 / elapsed.as_secs_f64());
        }
    }
  }


use clap::Parser;
use colored::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, AtomicU64, Ordering}};
use std::time::{Instant, SystemTime};
use walkdir::WalkDir;
use regex::Regex;

#[derive(Parser, Debug)]
#[command(name = "loco")]
#[command(about = "🚀 Advanced Line Counter")]
#[command(version = "0.2.0")]
struct Args {
    /// Path to analyze
    #[arg(short, long)]
    path: PathBuf,
    
    /// Verbose output with detailed statistics
    #[arg(short, long)]
    verbose: bool,
    
    /// Output format: text, json, csv, markdown, xml
    #[arg(short, long, default_value = "text")]
    format: String,
    
    /// Exclude directories (regex supported)
    #[arg(short, long)]
    exclude: Option<String>,
    
    /// Include only specific extensions
    #[arg(short, long)]
    include: Option<String>,
    
    /// Maximum file size to analyze (in MB)
    #[arg(long, default_value = "100")]
    max_size: u64,
    
    /// Number of threads (0 = auto)
    #[arg(short, long, default_value = "0")]
    threads: usize,
    
    /// Show progress bar
    #[arg(short = 'P', long)]
    progress: bool,
    
    /// Analyze code complexity
    #[arg(short = 'C', long)]
    complexity: bool,
    
    /// Show file size statistics
    #[arg(short = 'S', long)]
    size_stats: bool,
    
    /// Group by directory structure
    #[arg(short = 'G', long)]
    group_by_dir: bool,
    
    /// Show git statistics (if in git repo)
    #[arg(long)]
    git_stats: bool,
    
    /// Sort by: lines, files, size, name
    #[arg(long, default_value = "lines")]
    sort_by: String,
    
    /// Show top N languages only
    #[arg(long)]
    top: Option<usize>,
    
    /// Minimum lines to show language
    #[arg(long, default_value = "1")]
    min_lines: usize,
    
    /// Save output to file
    #[arg(short = 'o', long)]
    output: Option<PathBuf>,
    
    /// Show encoding information
    #[arg(long)]
    encoding: bool,
    
    /// Analyze file creation/modification times
    #[arg(long)]
    time_analysis: bool,
    
    /// Show duplicate code detection
    #[arg(long)]
    duplicates: bool,
    
    /// Export detailed report
    #[arg(long)]
    report: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct LanguageStats {
    total_lines: usize,
    code_lines: usize,
    comment_lines: usize,
    blank_lines: usize,
    files: usize,
    total_size: u64,
    avg_line_length: f64,
    max_line_length: usize,
    complexity_score: f64,
    functions: usize,
    classes: usize,
    imports: usize,
    todos: usize,
    fixmes: usize,
    file_sizes: Vec<u64>,
    creation_dates: Vec<SystemTime>,
    modification_dates: Vec<SystemTime>,
}

impl Default for LanguageStats {
    fn default() -> Self {
        Self {
            total_lines: 0,
            code_lines: 0,
            comment_lines: 0,
            blank_lines: 0,
            files: 0,
            total_size: 0,
            avg_line_length: 0.0,
            max_line_length: 0,
            complexity_score: 0.0,
            functions: 0,
            classes: 0,
            imports: 0,
            todos: 0,
            fixmes: 0,
            file_sizes: Vec::new(),
            creation_dates: Vec::new(),
            modification_dates: Vec::new(),
        }
    }
}

impl Clone for LanguageStats {
    fn clone(&self) -> Self {
        Self {
            total_lines: self.total_lines,
            code_lines: self.code_lines,
            comment_lines: self.comment_lines,
            blank_lines: self.blank_lines,
            files: self.files,
            total_size: self.total_size,
            avg_line_length: self.avg_line_length,
            max_line_length: self.max_line_length,
            complexity_score: self.complexity_score,
            functions: self.functions,
            classes: self.classes,
            imports: self.imports,
            todos: self.todos,
            fixmes: self.fixmes,
            file_sizes: self.file_sizes.clone(),
            creation_dates: self.creation_dates.clone(),
            modification_dates: self.modification_dates.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct FileInfo {
    path: PathBuf,
    language: String,
    lines: usize,
    size: u64,
    created: Option<SystemTime>,
    modified: Option<SystemTime>,
    encoding: String,
    complexity: f64,
}

#[derive(Debug, Clone, Serialize)]
struct DirectoryStats {
    path: PathBuf,
    languages: HashMap<String, LanguageStats>,
    total_files: usize,
    total_lines: usize,
    total_size: u64,
    depth: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ProjectStats {
    languages: HashMap<String, LanguageStats>,
    directories: Vec<DirectoryStats>,
    files: Vec<FileInfo>,
    total_files: usize,
    total_lines: usize,
    total_size: u64,
    analysis_time: f64,
    git_info: Option<GitStats>,
    duplicates: Vec<DuplicateInfo>,
    hotspots: Vec<FileInfo>,
}

#[derive(Debug, Clone, Serialize)]
struct GitStats {
    commits: usize,
    contributors: usize,
    first_commit: Option<SystemTime>,
    last_commit: Option<SystemTime>,
    branch: String,
    is_dirty: bool,
}

#[derive(Debug, Clone, Serialize)]
struct DuplicateInfo {
    content: String,
    files: Vec<PathBuf>,
    lines: usize,
}

struct LanguageConfig {
    single_line_comments: Vec<String>,
    multi_line_comments: Vec<(String, String)>,
    function_keywords: Vec<String>,
    class_keywords: Vec<String>,
    import_keywords: Vec<String>,
    complexity_keywords: Vec<String>,
}

impl LanguageConfig {
    fn get_config(extension: &str) -> Option<Self> {
        match extension {
            "rs" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["fn ".into(), "async fn".into()],
                class_keywords: vec!["struct ".into(), "enum ".into(), "trait ".into(), "impl ".into()],
                import_keywords: vec!["use ".into(), "extern ".into()],
                complexity_keywords: vec!["if".into(), "while".into(), "for".into(), "match".into(), "loop".into()],
            }),
            "py" => Some(Self {
                single_line_comments: vec!["#".into()],
                multi_line_comments: vec![("\"\"\"".into(), "\"\"\"".into()), ("'''".into(), "'''".into())],
                function_keywords: vec!["def ".into(), "async def ".into(), "lambda".into()],
                class_keywords: vec!["class ".into()],
                import_keywords: vec!["import ".into(), "from ".into()],
                complexity_keywords: vec!["if".into(), "while".into(), "for".into(), "try".into(), "except".into(), "with".into()],
            }),
            "js" | "ts" | "jsx" | "tsx" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["function ".into(), "=>".into(), "async ".into()],
                class_keywords: vec!["class ".into(), "interface ".into(), "type ".into()],
                import_keywords: vec!["import ".into(), "require(".into(), "export ".into()],
                complexity_keywords: vec!["if".into(), "while".into(), "for".into(), "switch".into(), "try".into(), "catch".into()],
            }),
            "java" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["public ".into(), "private ".into(), "protected ".into(), "static ".into()],
                class_keywords: vec!["class ".into(), "interface ".into(), "enum ".into(), "abstract ".into()],
                import_keywords: vec!["import ".into(), "package ".into()],
                complexity_keywords: vec!["if".into(), "while".into(), "for".into(), "switch".into(), "try".into(), "catch".into()],
            }),
            "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["int ".into(), "void ".into(), "char ".into(), "float ".into(), "double ".into()],
                class_keywords: vec!["class ".into(), "struct ".into(), "union ".into(), "enum ".into()],
                import_keywords: vec!["#include".into(), "#import".into()],
                complexity_keywords: vec!["if".into(), "while".into(), "for".into(), "switch".into(), "goto".into()],
            }),
            "go" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["func ".into()],
                class_keywords: vec!["type ".into(), "struct ".into(), "interface ".into()],
                import_keywords: vec!["import ".into(), "package ".into()],
                complexity_keywords: vec!["if".into(), "for".into(), "switch".into(), "select".into(), "go ".into()],
            }),
            _ => None,
        }
    }
}

fn get_language_name(extension: &str) -> String {
    match extension.to_lowercase().as_str() {
        "rs" => "Rust 🦀".to_string(),
        "py" | "pyw" | "pyi" => "Python 🐍".to_string(),
        "js" | "mjs" | "cjs" => "JavaScript 🟨".to_string(),
        "ts" => "TypeScript 🔷".to_string(),
        "jsx" => "React JSX ⚛️".to_string(),
        "tsx" => "React TypeScript ⚛️".to_string(),
        "java" => "Java ☕".to_string(),
        "kt" => "Kotlin 🎯".to_string(),
        "scala" => "Scala 🔥".to_string(),
        "c" => "C 🔧".to_string(),
        "cpp" | "cc" | "cxx" | "c++" => "C++ ⚡".to_string(),
        "cs" => "C# 💎".to_string(),
        "h" => "C Header 📋".to_string(),
        "hpp" | "hxx" => "C++ Header 📋".to_string(),
        "go" => "Go 🐹".to_string(),
        "rb" | "rake" => "Ruby 💎".to_string(),
        "php" => "PHP 🐘".to_string(),
        "html" | "htm" => "HTML 🌐".to_string(),
        "css" => "CSS 🎨".to_string(),
        "scss" => "SCSS 🎨".to_string(),
        "sass" => "Sass 🎨".to_string(),
        "less" => "Less 🎨".to_string(),
        "xml" | "svg" => "XML 📄".to_string(),
        "json" => "JSON 📊".to_string(),
        "yaml" | "yml" => "YAML 📝".to_string(),
        "toml" => "TOML ⚙️".to_string(),
        "md" | "markdown" => "Markdown 📖".to_string(),
        "sh" | "bash" => "Shell 🐚".to_string(),
        "sql" => "SQL 🗃️".to_string(),
        "lua" => "Lua 🌙".to_string(),
        "vim" => "Vim Script 📝".to_string(),
        "r" => "R 📊".to_string(),
        "swift" => "Swift 🏃‍♂️".to_string(),
        "dart" => "Dart 🎯".to_string(),
        "zig" => "Zig ⚡".to_string(),
        "haskell" | "hs" => "Haskell λ".to_string(),
        "elm" => "Elm 🌳".to_string(),
        "clojure" | "clj" => "Clojure 🔮".to_string(),
        "dockerfile" => "Dockerfile 🐳".to_string(),
        "makefile" => "Makefile 🔨".to_string(),
        "gitignore" => "Gitignore 📋".to_string(),
        _ => format!("Unknown ({})", extension),
    }
}

fn detect_encoding(file_path: &Path) -> String {
    if let Ok(bytes) = fs::read(file_path) {
        if bytes.len() >= 3 && &bytes[0..3] == b"\xEF\xBB\xBF" {
            return "UTF-8 BOM".to_string();
        }
        
        let mut _ascii_count = 0;
        let mut utf8_count = 0;
        
        for &byte in &bytes[..std::cmp::min(1024, bytes.len())] {
            if byte.is_ascii() {
                _ascii_count += 1;
            } else if byte & 0x80 != 0 {
                utf8_count += 1;
            }
        }
        
        if utf8_count == 0 {
            "ASCII".to_string()
        } else {
            "UTF-8".to_string()
        }
    } else {
        "Unknown".to_string()
    }
}

fn analyze_file_advanced(file_path: &Path, config: &LanguageConfig, args: &Args) -> Option<FileInfo> {
    let content = fs::read_to_string(file_path).ok()?;
    let metadata = fs::metadata(file_path).ok()?;
    
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    
    let mut code_lines = 0;
    let mut _comment_lines = 0;
    let mut _blank_lines = 0;
    let mut _functions = 0;
    let mut _classes = 0;
    let mut _imports = 0;
    let mut _todos = 0;
    let mut _fixmes = 0;
    let mut complexity_score = 0.0;
    let mut max_line_length = 0;
    let mut total_chars = 0;
    
    let mut in_multi_comment = false;
    let mut multi_comment_end = String::new();
    
    for line in &lines {
        let trimmed = line.trim();
        let line_length = line.len();
        max_line_length = max_line_length.max(line_length);
        total_chars += line_length;
        
        if trimmed.is_empty() {
            _blank_lines += 1;
            continue;
        }
        
        // Check for TODOs and FIXMEs
        if trimmed.to_uppercase().contains("TODO") { _todos += 1; }
        if trimmed.to_uppercase().contains("FIXME") { _fixmes += 1; }
        
        let mut is_comment = false;
        let mut line_content = trimmed;
        
        // Multi-line comment handling
        if in_multi_comment {
            is_comment = true;
            if let Some(end_pos) = line_content.find(&multi_comment_end) {
                in_multi_comment = false;
                line_content = &line_content[end_pos + multi_comment_end.len()..].trim();
                if line_content.is_empty() {
                    _comment_lines += 1;
                    continue;
                }
            } else {
                _comment_lines += 1;
                continue;
            }
        }
        
        // Check for multi-line comment start
        for (start, end) in &config.multi_line_comments {
            if let Some(start_pos) = line_content.find(start) {
                let before_comment = &line_content[..start_pos].trim();
                if !before_comment.is_empty() {
                    code_lines += 1;
                    break;
                }
                
                if let Some(end_pos) = line_content[start_pos + start.len()..].find(end) {
                    let after_comment = &line_content[start_pos + start.len() + end_pos + end.len()..].trim();
                    is_comment = after_comment.is_empty();
                } else {
                    in_multi_comment = true;
                    multi_comment_end = end.clone();
                    is_comment = true;
                }
                break;
            }
        }
        
        if !is_comment {
            // Check for single-line comments
            for comment_start in &config.single_line_comments {
                if let Some(pos) = line_content.find(comment_start) {
                    let before_comment = &line_content[..pos].trim();
                    is_comment = before_comment.is_empty();
                    if !is_comment {
                        code_lines += 1;
                    }
                    break;
                }
            }
            
            if !is_comment {
                code_lines += 1;
                
                // Analyze code patterns
                for keyword in &config.function_keywords {
                    if line_content.contains(keyword) { _functions += 1; break; }
                }
                for keyword in &config.class_keywords {
                    if line_content.contains(keyword) { _classes += 1; break; }
                }
                for keyword in &config.import_keywords {
                    if line_content.contains(keyword) { _imports += 1; break; }
                }
                for keyword in &config.complexity_keywords {
                    if line_content.contains(keyword) { complexity_score += 1.0; }
                }
            }
        }
        
        if is_comment {
            _comment_lines += 1;
        }
    }
    
    let _avg_line_length = if total_lines > 0 {
        total_chars as f64 / total_lines as f64
    } else { 0.0 };
    
    // Calculate complexity score
    complexity_score = complexity_score / (code_lines.max(1) as f64);
    
    let encoding = if args.encoding {
        detect_encoding(file_path)
    } else {
        "N/A".to_string()
    };
    
    let created = if args.time_analysis {
        metadata.created().ok()
    } else { None };
    
    let modified = if args.time_analysis {
        metadata.modified().ok()
    } else { None };
    
    let extension = file_path.extension()?.to_str()?;
    let language = get_language_name(extension);
    
    Some(FileInfo {
        path: file_path.to_path_buf(),
        language,
        lines: total_lines,
        size: metadata.len(),
        created,
        modified,
        encoding,
        complexity: complexity_score,
    })
}

fn collect_files_advanced(path: &Path, args: &Args) -> Vec<PathBuf> {
    let mut files = Vec::new();
    
    let exclude_regex = if let Some(ref exclude) = args.exclude {
        Some(Regex::new(exclude).unwrap_or_else(|_| Regex::new("^$").unwrap()))
    } else { None };
    
    let include_exts: Option<Vec<String>> = args.include.as_ref().map(|s| 
        s.split(',').map(|ext| ext.trim().to_lowercase()).collect()
    );
    
    let default_excludes = [
        "target", "node_modules", ".git", "build", "dist", "__pycache__", 
        ".cargo", ".next", ".nuxt", "vendor", "coverage", ".pytest_cache",
        ".vscode", ".idea", "bin", "obj", ".vs", "packages", ".svn", ".hg",
        "deps", "tmp", "temp", "cache", ".cache", "logs"
    ];
    
    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let file_path = entry.path();
        
        // Size check
        if let Ok(metadata) = file_path.metadata() {
            if metadata.len() > args.max_size * 1024 * 1024 {
                continue;
            }
        }
        
        let path_str = file_path.to_string_lossy();
        
        // Regex exclude check
        if let Some(ref regex) = exclude_regex {
            if regex.is_match(&path_str) {
                continue;
            }
        }
        
        // Default excludes
        let mut should_exclude = false;
        for exclude in &default_excludes {
            if path_str.contains(&format!("/{}/", exclude)) || 
               path_str.contains(&format!("\\{}\\", exclude)) {
                should_exclude = true;
                break;
            }
        }
        
        if should_exclude { continue; }
        
        // Extension filter
        if let Some(ref include_exts) = include_exts {
            if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                if !include_exts.contains(&ext.to_lowercase()) {
                    continue;
                }
            } else {
                continue;
            }
        }
        
        files.push(file_path.to_path_buf());
    }
    
    files
}

fn print_advanced_results(stats: &ProjectStats, args: &Args) {
    println!("{}", "🚀 LOCO - Advanced Code Analysis Report".bright_cyan().bold());
    println!("{}", "=".repeat(80).bright_black());
    
    if let Some(ref git_stats) = stats.git_info {
        println!("\n{} Git Repository Information", "📂".bright_green().bold());
        println!("  {} Branch: {}", "🌿", git_stats.branch.bright_white());
        println!("  {} Commits: {}", "📝", git_stats.commits.to_string().bright_white());
        println!("  {} Contributors: {}", "👥", git_stats.contributors.to_string().bright_white());
        println!("  {} Status: {}", "🔄", 
            if git_stats.is_dirty { "Modified".bright_yellow() } else { "Clean".bright_green() });
    }
    
    println!("\n{} Project Overview", "📊".bright_magenta().bold());
    println!("  {} {} total files", "📁", stats.total_files.to_string().bright_white());
    println!("  {} {} total lines", "📏", stats.total_lines.to_string().bright_white());
    println!("  {} {:.2} MB total size", "💾", (stats.total_size as f64 / 1_048_576.0).to_string().bright_white());
    println!("  {} {:.3}s analysis time", "⚡", stats.analysis_time.to_string().bright_white());
    
    let mut sorted_languages: Vec<_> = stats.languages.iter().collect();
    
    match args.sort_by.as_str() {
        "files" => sorted_languages.sort_by(|a, b| b.1.files.cmp(&a.1.files)),
        "size" => sorted_languages.sort_by(|a, b| b.1.total_size.cmp(&a.1.total_size)),
        "name" => sorted_languages.sort_by(|a, b| a.0.cmp(b.0)),
        _ => sorted_languages.sort_by(|a, b| b.1.total_lines.cmp(&a.1.total_lines)),
    }
    
    if let Some(top) = args.top {
        sorted_languages.truncate(top);
    }
    
    println!("\n{} Language Statistics", "🔤".bright_blue().bold());
    println!("{}", "-".repeat(80).bright_black());
    
    for (language, lang_stats) in &sorted_languages {
        let total_lines = lang_stats.total_lines;
        if total_lines < args.min_lines { continue; }
        
        let code_lines = lang_stats.code_lines;
        let comment_lines = lang_stats.comment_lines;
        let blank_lines = lang_stats.blank_lines;
        let files = lang_stats.files;
        let size = lang_stats.total_size;
        
        println!("\n{} {}", "▶️", language.bright_white().bold());
        println!("  {} {} files ({:.1}%)", "📄", files.to_string().bright_cyan(),
            (files as f64 / stats.total_files as f64 * 100.0).to_string().bright_white());
        println!("  {} {} lines ({:.1}%)", "📊", total_lines.to_string().bright_green(),
            (total_lines as f64 / stats.total_lines as f64 * 100.0).to_string().bright_white());
        println!("  {} {} code | {} {} comments | {} {} blank", 
            "💻", code_lines.to_string().bright_green(),
            "💬", comment_lines.to_string().bright_blue(),
            "⬜", blank_lines.to_string().bright_black());
        
        if args.size_stats {
            println!("  {} {:.2} MB ({:.1} KB/file)", "💾", 
                size as f64 / 1_048_576.0,
                size as f64 / 1024.0 / files as f64);
        }
        
        if args.complexity {
            println!("  {} {:.2} complexity score", "🧮", lang_stats.complexity_score);
            println!("  {} {} functions | {} {} classes | {} {} imports", 
                "🔧", lang_stats.functions.to_string().bright_yellow(),
                "🏗️", lang_stats.classes.to_string().bright_magenta(),
                "📦", lang_stats.imports.to_string().bright_cyan());
            
            if lang_stats.todos > 0 || lang_stats.fixmes > 0 {
                println!("  {} {} TODOs | {} {} FIXMEs", 
                    "📝", lang_stats.todos.to_string().bright_yellow(),
                    "🔧", lang_stats.fixmes.to_string().bright_red());
            }
        }
        
        if args.verbose {
            let code_ratio = if total_lines > 0 {
                code_lines as f64 / total_lines as f64 * 100.0
            } else { 0.0 };
            println!("  {} {:.1}% code density | {:.1} avg line length | {} max line", 
                "📈", code_ratio,
                lang_stats.avg_line_length,
                lang_stats.max_line_length.to_string().bright_white());
        }
    }
    
    if args.group_by_dir && !stats.directories.is_empty() {
        println!("\n{} Directory Analysis", "📂".bright_purple().bold());
        println!("{}", "-".repeat(80).bright_black());
        
        for dir_stats in &stats.directories[..std::cmp::min(10, stats.directories.len())] {
            println!("\n{} {}", "📁", dir_stats.path.display().to_string().bright_white());
            println!("  {} {} files | {} {} lines | {} {:.1} MB", 
                "📊", dir_stats.total_files.to_string().bright_cyan(),
                "📏", dir_stats.total_lines.to_string().bright_green(),
                "💾", dir_stats.total_size as f64 / 1_048_576.0);
        }
    }
    
    if !stats.duplicates.is_empty() {
        println!("\n{} Duplicate Code Detected", "⚠️".bright_yellow().bold());
        for (i, dup) in stats.duplicates.iter().take(5).enumerate() {
            println!("  {} Duplicate #{}: {} lines in {} files", 
                "🔍", i+1, dup.lines.to_string().bright_red(), dup.files.len().to_string().bright_white());
        }
    }
    
    if !stats.hotspots.is_empty() {
        println!("\n{} Largest Files (Top 10)", "🔥".bright_red().bold());
        for (i, file) in stats.hotspots.iter().take(10).enumerate() {
            println!("  {}. {} ({} lines, {:.1} KB)", 
                i+1, file.path.display().to_string().bright_white(),
                file.lines.to_string().bright_cyan(),
                file.size as f64 / 1024.0);
        }
    }
    
    println!("\n{} Performance Metrics", "⚡".bright_green().bold());
    println!("{}", "-".repeat(80).bright_black());
    println!("  {} {:.2} files/second", "🚀", stats.total_files as f64 / stats.analysis_time);
    println!("  {} {:.0} lines/second", "📈", stats.total_lines as f64 / stats.analysis_time);
    println!("  {} {:.2} MB/second", "💾", stats.total_size as f64 / 1_048_576.0 / stats.analysis_time);
}

fn main() {
    let args = Args::parse();
    
    if !args.path.exists() {
        eprintln!("{} Path does not exist: {}", "❌".bright_red(), args.path.display());
        std::process::exit(1);
    }
    
    // Set thread count
    if args.threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.threads)
            .build_global()
            .unwrap();
    }
    
    println!("{} Initializing LOCO Advanced Analysis Engine...", "🚀".bright_cyan());
    println!("{} Target: {}", "🎯", args.path.display().to_string().bright_white());
    
    let start_time = Instant::now();
    let files = collect_files_advanced(&args.path, &args);
    
    if files.is_empty() {
        println!("{} No files found matching criteria.", "⚠️".bright_yellow());
        return;
    }
    
    println!("{} Processing {} files with {} threads...", 
        "⚙️".bright_blue(), 
        files.len().to_string().bright_white(),
        rayon::current_num_threads().to_string().bright_white());
    
    let languages: Arc<Mutex<HashMap<String, LanguageStats>>> = Arc::new(Mutex::new(HashMap::new()));
    let all_files: Arc<Mutex<Vec<FileInfo>>> = Arc::new(Mutex::new(Vec::new()));
    let processed = Arc::new(AtomicUsize::new(0));
    
    files.par_iter().for_each(|file_path| {
        if let Some(extension) = file_path.extension().and_then(|e| e.to_str()) {
            if let Some(config) = LanguageConfig::get_config(extension) {
                if let Some(file_info) = analyze_file_advanced(file_path, &config, &args) {
                    {
                        let mut all_files_guard = all_files.lock().unwrap();
                        all_files_guard.push(file_info.clone());
                    }
                    
                    {
                        let mut languages_guard = languages.lock().unwrap();
                        let entry = languages_guard.entry(file_info.language.clone())
                            .or_insert_with(LanguageStats::default);
                        
                        entry.total_lines += file_info.lines;
                        entry.files += 1;
                        entry.total_size += file_info.size;
                    }
                }
            }
        }
        
        let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
        if args.progress && current % 100 == 0 {
            print!("\r{} Progress: {}/{} files processed", "🔄".bright_blue(), current, files.len());
            io::stdout().flush().unwrap();
        }
    });
    
    if args.progress {
        println!("\r{} Completed processing {} files!", "✅".bright_green(), files.len());
    }
    
    let final_languages = Arc::try_unwrap(languages).unwrap().into_inner().unwrap();
    let final_files = Arc::try_unwrap(all_files).unwrap().into_inner().unwrap();
    
    // Calculate hotspots (largest files)
    let mut hotspots = final_files.clone();
    hotspots.sort_by(|a, b| b.lines.cmp(&a.lines));
    hotspots.truncate(10);
    
    let analysis_time = start_time.elapsed().as_secs_f64();
    
    let project_stats = ProjectStats {
        languages: final_languages,
        directories: Vec::new(), // TODO: Implement directory grouping
        files: final_files.clone(),
        total_files: final_files.len(),
        total_lines: final_files.iter().map(|f| f.lines).sum(),
        total_size: final_files.iter().map(|f| f.size).sum(),
        analysis_time,
        git_info: None, // TODO: Implement git analysis
        duplicates: Vec::new(), // TODO: Implement duplicate detection
        hotspots,
    };
    
    match args.format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&project_stats).unwrap();
            if let Some(output_path) = &args.output {
                fs::write(output_path, &json).unwrap();
                println!("Results saved to: {}", output_path.display());
            } else {
                println!("{}", json);
            }
        },
        _ => {
            print_advanced_results(&project_stats, &args);
        }
    }
    
    println!("\n{} Analysis completed successfully! 🎉", "✅".bright_green().bold());
                        }

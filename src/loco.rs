
use clap::Parser;
use colored::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;
use regex::Regex;
use std::process::Command;

#[derive(Parser, Debug)]
#[command(name = "loco")]
#[command(about = "🚀 Advanced Line Counter")]
#[command(version = "0.3.0")]
struct Args {
    /// Path to analyze
    #[arg(short, long)]
    path: PathBuf,

    /// Verbose output with detailed statistics
    #[arg(short, long)]
    verbose: bool,

    /// Output format: text, json, csv, markdown, xml, html
    #[arg(short, long, default_value = "text")]
    format: String,

    /// Exclude directories (regex supported)
    #[arg(short, long)]
    exclude: Option<String>,

    /// Include only specific extensions (comma-separated)
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

    /// Export detailed report (HTML/Markdown)
    #[arg(long)]
    report: bool,

    /// Show top files by metric (lines, complexity, todos, size)
    #[arg(long)]
    top_files: Option<String>,

    /// Show hotspot detection (risky files)
    #[arg(long)]
    hotspots: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    code_percentage: f64,
    comment_percentage: f64,
    blank_percentage: f64,
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
            code_percentage: 0.0,
            comment_percentage: 0.0,
            blank_percentage: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct FileInfo {
    path: PathBuf,
    language: String,
    lines: usize,
    size: u64,
    encoding: String,
    complexity: f64,
    created: Option<u64>,
    modified: Option<u64>,
    todos: usize,
    fixmes: usize,
}

#[derive(Debug, Clone, Serialize)]
struct GitStats {
    total_commits: usize,
    contributors: usize,
    last_commit_date: Option<String>,
    lines_added: usize,
    lines_deleted: usize,
    branch: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProjectStats {
    languages: HashMap<String, LanguageStats>,
    total_files: usize,
    total_lines: usize,
    total_size: u64,
    analysis_time: f64,
    git_info: Option<GitStats>,
    creation_dates: Vec<u64>,
    modification_dates: Vec<u64>,
    files_info: Vec<FileInfo>,
    hotspots: Vec<FileInfo>,
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
                function_keywords: vec!["fn ".into()],
                class_keywords: vec!["struct ".into(), "enum ".into(), "trait ".into(), "impl ".into()],
                import_keywords: vec!["use ".into(), "extern ".into()],
                complexity_keywords: vec!["if ".into(), "while ".into(), "for ".into(), "match ".into(), "loop ".into()],
            }),
            "py" => Some(Self {
                single_line_comments: vec!["#".into()],
                multi_line_comments: vec![("\"\"\"".into(), "\"\"\"".into()), ("'''".into(), "'''".into())],
                function_keywords: vec!["def ".into(), "async def ".into(), "lambda ".into()],
                class_keywords: vec!["class ".into()],
                import_keywords: vec!["import ".into(), "from ".into()],
                complexity_keywords: vec!["if ".into(), "while ".into(), "for ".into(), "try ".into(), "except ".into(), "with ".into()],
            }),
            "js" | "ts" | "jsx" | "tsx" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["function ".into(), "=>".into(), "async ".into()],
                class_keywords: vec!["class ".into(), "interface ".into(), "type ".into()],
                import_keywords: vec!["import ".into(), "require(".into(), "export ".into()],
                complexity_keywords: vec!["if ".into(), "while ".into(), "for ".into(), "switch ".into(), "try ".into(), "catch ".into()],
            }),
            "java" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["public ".into(), "private ".into(), "protected ".into()],
                class_keywords: vec!["class ".into(), "interface ".into(), "enum ".into()],
                import_keywords: vec!["import ".into(), "package ".into()],
                complexity_keywords: vec!["if ".into(), "while ".into(), "for ".into(), "switch ".into(), "try ".into(), "catch ".into()],
            }),
            "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["int ".into(), "void ".into(), "char ".into(), "float ".into(), "double ".into()],
                class_keywords: vec!["class ".into(), "struct ".into(), "union ".into(), "enum ".into()],
                import_keywords: vec!["#include".into(), "#import".into()],
                complexity_keywords: vec!["if ".into(), "while ".into(), "for ".into(), "switch ".into()],
            }),
            "go" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["func ".into()],
                class_keywords: vec!["type ".into(), "struct ".into(), "interface ".into()],
                import_keywords: vec!["import ".into(), "package ".into()],
                complexity_keywords: vec!["if ".into(), "for ".into(), "switch ".into(), "select ".into()],
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
        "c" => "C 🔧".to_string(),
        "cpp" | "cc" | "cxx" | "c++" => "C++ ⚡".to_string(),
        "h" => "C Header 📋".to_string(),
        "hpp" | "hxx" => "C++ Header 📋".to_string(),
        "go" => "Go 🐹".to_string(),
        "html" | "htm" => "HTML 🌐".to_string(),
        "css" => "CSS 🎨".to_string(),
        "json" => "JSON 📊".to_string(),
        "yaml" | "yml" => "YAML 📝".to_string(),
        "toml" => "TOML ⚙️".to_string(),
        "md" | "markdown" => "Markdown 📖".to_string(),
        "sh" | "bash" => "Shell 🐚".to_string(),
        "sql" => "SQL 🗃️".to_string(),
        _ => format!("Unknown ({})", extension),
    }
}

fn detect_encoding_advanced(file_path: &Path) -> String {
    if let Ok(bytes) = fs::read(file_path) {
        if bytes.is_empty() {
            return "Empty".to_string();
        }

        // Check BOM
        if bytes.len() >= 3 && &bytes[0..3] == b"\xEF\xBB\xBF" {
            return "UTF-8 BOM".to_string();
        }
        if bytes.len() >= 2 && &bytes[0..2] == b"\xFF\xFE" {
            return "UTF-16 LE".to_string();
        }
        if bytes.len() >= 2 && &bytes[0..2] == b"\xFE\xFF" {
            return "UTF-16 BE".to_string();
        }

        // Sample analysis
        let sample_size = std::cmp::min(4096, bytes.len());
        let sample = &bytes[0..sample_size];

        let mut ascii_count = 0;
        let mut utf8_valid = true;
        let mut high_bytes = 0;

        // Check if valid UTF-8
        if let Err(_) = std::str::from_utf8(sample) {
            utf8_valid = false;
        }

        for &byte in sample {
            if byte.is_ascii() {
                ascii_count += 1;
            } else {
                high_bytes += 1;
            }
        }

        let ascii_ratio = ascii_count as f64 / sample.len() as f64;

        if ascii_ratio == 1.0 {
            "ASCII".to_string()
        } else if utf8_valid {
            "UTF-8".to_string()
        } else if high_bytes > 0 {
            "Binary/Unknown".to_string()
        } else {
            "ISO-8859-1".to_string()
        }
    } else {
        "Unreadable".to_string()
    }
}

fn get_file_times(file_path: &Path) -> (Option<u64>, Option<u64>) {
    if let Ok(metadata) = fs::metadata(file_path) {
        let created = metadata.created().ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        let modified = metadata.modified().ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        (created, modified)
    } else {
        (None, None)
    }
}

fn get_git_stats(path: &Path) -> Option<GitStats> {
    // Check if we're in a git repository
    let git_dir = path.join(".git");
    if !git_dir.exists() {
        return None;
    }

    let mut git_stats = GitStats {
        total_commits: 0,
        contributors: 0,
        last_commit_date: None,
        lines_added: 0,
        lines_deleted: 0,
        branch: None,
    };

    // Get total commits
    if let Ok(output) = Command::new("git")
        .args(&["rev-list", "--count", "HEAD"])
        .current_dir(path)
        .output()
    {
        if let Ok(count_str) = String::from_utf8(output.stdout) {
            git_stats.total_commits = count_str.trim().parse().unwrap_or(0);
        }
    }

    // Get contributors count
    if let Ok(output) = Command::new("git")
        .args(&["shortlog", "-sn"])
        .current_dir(path)
        .output()
    {
        if let Ok(contributors_str) = String::from_utf8(output.stdout) {
            git_stats.contributors = contributors_str.lines().count();
        }
    }

    // Get last commit date
    if let Ok(output) = Command::new("git")
        .args(&["log", "-1", "--format=%cd", "--date=short"])
        .current_dir(path)
        .output()
    {
        if let Ok(date_str) = String::from_utf8(output.stdout) {
            git_stats.last_commit_date = Some(date_str.trim().to_string());
        }
    }

    // Get current branch
    if let Ok(output) = Command::new("git")
        .args(&["branch", "--show-current"])
        .current_dir(path)
        .output()
    {
        if let Ok(branch_str) = String::from_utf8(output.stdout) {
            git_stats.branch = Some(branch_str.trim().to_string());
        }
    }

    // Get lines added/deleted (last 100 commits)
    if let Ok(output) = Command::new("git")
        .args(&["log", "-100", "--numstat", "--pretty=format:"])
        .current_dir(path)
        .output()
    {
        if let Ok(stats_str) = String::from_utf8(output.stdout) {
            for line in stats_str.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let (Ok(added), Ok(deleted)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                        git_stats.lines_added += added;
                        git_stats.lines_deleted += deleted;
                    }
                }
            }
        }
    }

    Some(git_stats)
}

fn analyze_file_advanced(file_path: &Path, config: &LanguageConfig, args: &Args) -> Option<(LanguageStats, FileInfo)> {
    let content = fs::read_to_string(file_path).ok()?;
    let metadata = fs::metadata(file_path).ok()?;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let mut code_lines = 0;
    let mut comment_lines = 0;
    let mut blank_lines = 0;
    let mut functions = 0;
    let mut classes = 0;
    let mut imports = 0;
    let mut todos = 0;
    let mut fixmes = 0;
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
            blank_lines += 1;
            continue;
        }

        // Check for TODOs and FIXMEs
        if trimmed.to_uppercase().contains("TODO") { todos += 1; }
        if trimmed.to_uppercase().contains("FIXME") { fixmes += 1; }

        let mut is_comment = false;
        let mut line_content = trimmed.to_string();

        // Multi-line comment handling
        if in_multi_comment {
            comment_lines += 1;
            if let Some(end_pos) = line_content.find(&multi_comment_end) {
                in_multi_comment = false;
                line_content = line_content[end_pos + multi_comment_end.len()..].trim().to_string();
                if line_content.is_empty() {
                    continue;
                }
            } else {
                continue;
            }
        }

        // Check for multi-line comment start
        for (start, end) in &config.multi_line_comments {
            if let Some(start_pos) = line_content.find(start) {
                let before_comment = &line_content[..start_pos].trim();

                if let Some(end_pos) = line_content[start_pos + start.len()..].find(end) {
                    let after_comment = &line_content[start_pos + start.len() + end_pos + end.len()..].trim();
                    if !before_comment.is_empty() || !after_comment.is_empty() {
                        code_lines += 1;
                    } else {
                        comment_lines += 1;
                    }
                    is_comment = true;
                } else {
                    in_multi_comment = true;
                    multi_comment_end = end.clone();
                    if !before_comment.is_empty() {
                        code_lines += 1;
                    } else {
                        comment_lines += 1;
                    }
                    is_comment = true;
                }
                break;
            }
        }

        if !is_comment {
            // Check for single-line comments
            let mut found_comment = false;
            for comment_start in &config.single_line_comments {
                if let Some(pos) = line_content.find(comment_start) {
                    let before_comment = &line_content[..pos].trim();
                    if before_comment.is_empty() {
                        comment_lines += 1;
                        found_comment = true;
                    } else {
                        code_lines += 1;
                    }
                    break;
                }
            }

            if !found_comment {
                code_lines += 1;

                // Analyze code patterns
                for keyword in &config.function_keywords {
                    if line_content.contains(keyword) { functions += 1; break; }
                }
                for keyword in &config.class_keywords {
                    if line_content.contains(keyword) { classes += 1; break; }
                }
                for keyword in &config.import_keywords {
                    if line_content.contains(keyword) { imports += 1; break; }
                }
                for keyword in &config.complexity_keywords {
                    if line_content.contains(keyword) { complexity_score += 1.0; }
                }
            }
        }
    }

    let avg_line_length = if total_lines > 0 {
        total_chars as f64 / total_lines as f64
    } else { 0.0 };

    // Calculate complexity score
    complexity_score = if code_lines > 0 {
        complexity_score / code_lines as f64
    } else { 0.0 };

    // Calculate percentages
    let code_percentage = if total_lines > 0 { code_lines as f64 / total_lines as f64 * 100.0 } else { 0.0 };
    let comment_percentage = if total_lines > 0 { comment_lines as f64 / total_lines as f64 * 100.0 } else { 0.0 };
    let blank_percentage = if total_lines > 0 { blank_lines as f64 / total_lines as f64 * 100.0 } else { 0.0 };

    let (created, modified) = get_file_times(file_path);
    let encoding = if args.encoding {
        detect_encoding_advanced(file_path)
    } else {
        "N/A".to_string()
    };

    let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let language = get_language_name(extension);

    let lang_stats = LanguageStats {
        total_lines,
        code_lines,
        comment_lines,
        blank_lines,
        files: 1,
        total_size: metadata.len(),
        avg_line_length,
        max_line_length,
        complexity_score,
        functions,
        classes,
        imports,
        todos,
        fixmes,
        code_percentage,
        comment_percentage,
        blank_percentage,
    };

    let file_info = FileInfo {
        path: file_path.to_path_buf(),
        language,
        lines: total_lines,
        size: metadata.len(),
        encoding,
        complexity: complexity_score,
        created,
        modified,
        todos,
        fixmes,
    };

    Some((lang_stats, file_info))
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
        } else {
            // Only include files with known extensions if no filter is specified
            if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                if LanguageConfig::get_config(ext).is_none() {
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

fn detect_hotspots(files_info: &[FileInfo]) -> Vec<FileInfo> {
    let mut hotspots = Vec::new();

    // Define thresholds for hotspot detection
    let large_file_threshold = 1000; // lines
    let high_complexity_threshold = 0.3;
    let high_todos_threshold = 10;
    let large_size_threshold = 100 * 1024; // 100KB

    for file in files_info {
        let mut risk_score = 0;

        if file.lines > large_file_threshold { risk_score += 2; }
        if file.complexity > high_complexity_threshold { risk_score += 3; }
        if file.todos > high_todos_threshold { risk_score += 2; }
        if file.size > large_size_threshold { risk_score += 1; }

        if risk_score >= 3 {
            hotspots.push(file.clone());
        }
    }

    // Sort by risk (combination of complexity and size)
    hotspots.sort_by(|a, b| {
        let risk_a = a.complexity + (a.size as f64 / 1000.0) + (a.todos as f64 / 10.0);
        let risk_b = b.complexity + (b.size as f64 / 1000.0) + (b.todos as f64 / 10.0);
        risk_b.partial_cmp(&risk_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    hotspots.truncate(10); // Top 10 hotspots
    hotspots
}

fn generate_html_report(stats: &ProjectStats, args: &Args) -> String {
    format!(r#"
<!DOCTYPE html>
<html>
<head>
    <title>LOCO Code Analysis Report</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 20px; background: #f5f5f5; }}
        .container {{ max-width: 1200px; margin: 0 auto; background: white; padding: 20px; border-radius: 10px; }}
        .header {{ text-align: center; color: #2c3e50; margin-bottom: 30px; }}
        .stats-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(250px, 1fr)); gap: 20px; margin: 20px 0; }}
        .stat-card {{ background: #ecf0f1; padding: 15px; border-radius: 8px; text-align: center; }}
        .stat-value {{ font-size: 2em; font-weight: bold; color: #27ae60; }}
        .language-table {{ width: 100%; border-collapse: collapse; margin: 20px 0; }}
        .language-table th, .language-table td {{ border: 1px solid #ddd; padding: 12px; text-align: left; }}
        .language-table th {{ background-color: #34495e; color: white; }}
        .language-table tr:nth-child(even) {{ background-color: #f9f9f9; }}
        .progress-bar {{ background: #ecf0f1; border-radius: 10px; overflow: hidden; height: 20px; margin: 5px 0; }}
        .progress-fill {{ height: 100%; background: linear-gradient(90deg, #3498db, #2980b9); transition: width 0.3s; }}
        .section {{ margin: 30px 0; }}
        .hotspot {{ background: #e74c3c; color: white; padding: 10px; margin: 5px 0; border-radius: 5px; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🚀 LOCO Code Analysis Report</h1>
            <p>Generated on {}</p>
        </div>

        <div class="stats-grid">
            <div class="stat-card">
                <div class="stat-value">{}</div>
                <div>Total Files</div>
            </div>
            <div class="stat-card">
                <div class="stat-value">{}</div>
                <div>Total Lines</div>
            </div>
            <div class="stat-card">
                <div class="stat-value">{:.2}</div>
                <div>Size (MB)</div>
            </div>
            <div class="stat-card">
                <div class="stat-value">{:.3}</div>
                <div>Analysis Time (s)</div>
            </div>
        </div>

        <div class="section">
            <h2>📊 Language Breakdown</h2>
            <table class="language-table">
                <tr>
                    <th>Language</th>
                    <th>Files</th>
                    <th>Lines</th>
                    <th>Code %</th>
                    <th>Comments %</th>
                    <th>Blank %</th>
                    <th>Complexity</th>
                </tr>
                {}
            </table>
        </div>

        {}

        {}
    </div>
</body>
</html>
"#,
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        stats.total_files,
        stats.total_lines,
        stats.total_size as f64 / 1_048_576.0,
        stats.analysis_time,
        generate_language_rows(stats),
        generate_git_section(stats),
        generate_hotspots_section(stats)
    )
}

fn generate_language_rows(stats: &ProjectStats) -> String {
    let mut rows = String::new();
    let mut sorted_languages: Vec<_> = stats.languages.iter().collect();
    sorted_languages.sort_by(|a, b| b.1.total_lines.cmp(&a.1.total_lines));

    for (language, lang_stats) in sorted_languages {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:.1}%</td><td>{:.1}%</td><td>{:.1}%</td><td>{:.3}</td></tr>",
            language,
            lang_stats.files,
            lang_stats.total_lines,
            lang_stats.code_percentage,
            lang_stats.comment_percentage,
            lang_stats.blank_percentage,
            lang_stats.complexity_score
        ));
    }
    rows
}

fn generate_git_section(stats: &ProjectStats) -> String {
    if let Some(ref git_info) = stats.git_info {
        format!(r#"
        <div class="section">
            <h2>🔄 Git Statistics</h2>
            <div class="stats-grid">
                <div class="stat-card">
                    <div class="stat-value">{}</div>
                    <div>Total Commits</div>
                </div>
                <div class="stat-card">
                    <div class="stat-value">{}</div>
                    <div>Contributors</div>
                </div>
                <div class="stat-card">
                    <div class="stat-value">{}</div>
                    <div>Lines Added</div>
                </div>
                <div class="stat-card">
                    <div class="stat-value">{}</div>
                    <div>Lines Deleted</div>
                </div>
            </div>
            <p><strong>Last Commit:</strong> {}</p>
            <p><strong>Current Branch:</strong> {}</p>
        </div>
        "#,
        git_info.total_commits,
        git_info.contributors,
        git_info.lines_added,
        git_info.lines_deleted,
        git_info.last_commit_date.as_ref().unwrap_or(&"Unknown".to_string()),
        git_info.branch.as_ref().unwrap_or(&"Unknown".to_string())
        )
    } else {
        String::new()
    }
}

fn generate_hotspots_section(stats: &ProjectStats) -> String {
    if !stats.hotspots.is_empty() {
        let mut section = String::from(r#"
        <div class="section">
            <h2>🔥 Code Hotspots (Risk Files)</h2>
        "#);

        for hotspot in &stats.hotspots {
            section.push_str(&format!(
                r#"<div class="hotspot">
                    📁 {} | 📏 {} lines | 🧮 {:.3} complexity | 📝 {} TODOs | 💾 {:.1} KB
                </div>"#,
                hotspot.path.display(),
                hotspot.lines,
                hotspot.complexity,
                hotspot.todos,
                hotspot.size as f64 / 1024.0
            ));
        }
        
        section.push_str("</div>");
        section
    } else {
        String::new()
    }
}

fn show_top_files(stats: &ProjectStats, metric: &str) {
    let mut files = stats.files_info.clone();

    match metric {
        "lines" => files.sort_by(|a, b| b.lines.cmp(&a.lines)),
        "complexity" => files.sort_by(|a, b| b.complexity.partial_cmp(&a.complexity).unwrap_or(std::cmp::Ordering::Equal)),
        "todos" => files.sort_by(|a, b| b.todos.cmp(&a.todos)),
        "size" => files.sort_by(|a, b| b.size.cmp(&a.size)),
        _ => return,
    }

    files.truncate(10);

    println!("\n{} Top 10 Files by {}", "🏆".bright_yellow().bold(), metric.to_uppercase());
    println!("{}", "-".repeat(80).bright_black());

    for (i, file) in files.iter().enumerate() {
        let value = match metric {
            "lines" => file.lines.to_string(),
            "complexity" => format!("{:.3}", file.complexity),
            "todos" => file.todos.to_string(),
            "size" => format!("{:.1} KB", file.size as f64 / 1024.0),
            _ => "0".to_string(),
        };

        println!("  {}. {} | {} {}", 
            (i + 1).to_string().bright_white(),
            file.path.display().to_string().bright_cyan(),
            value.bright_green(),
            metric
        );
    }
}

fn print_advanced_results(stats: &ProjectStats, args: &Args) {
    println!("{}", "🚀 LOCO - Advanced Code Analysis Report".bright_cyan().bold());
    println!("{}", "=".repeat(80).bright_black());

    println!("\n{} Project Overview", "📊".bright_magenta().bold());
    println!("  📁 {} total files", stats.total_files.to_string().bright_white());
    println!("  📏 {} total lines", stats.total_lines.to_string().bright_white());
    println!("  💾 {:.2} MB total size", (stats.total_size as f64 / 1_048_576.0).to_string().bright_white());
    
    // Only show performance metrics for larger projects
    if stats.total_files > 50 || stats.analysis_time > 1.0 {
        println!("  ⚡ {:.3}s analysis time", stats.analysis_time.to_string().bright_white());
        println!("  🚀 {:.1} files/sec", (stats.total_files as f64 / stats.analysis_time).to_string().bright_cyan());
        println!("  📈 {:.0} lines/sec", (stats.total_lines as f64 / stats.analysis_time).to_string().bright_cyan());
    }

    // Git statistics
    if let Some(ref git_info) = stats.git_info {
        println!("\n{} Git Repository Info", "🔄".bright_blue().bold());
        println!("  📊 {} total commits", git_info.total_commits.to_string().bright_white());
        println!("  👥 {} contributors", git_info.contributors.to_string().bright_white());
        if let Some(ref last_commit) = git_info.last_commit_date {
            println!("  📅 Last commit: {}", last_commit.bright_white());
        }
        if let Some(ref branch) = git_info.branch {
            println!("  🌿 Current branch: {}", branch.bright_white());
        }
        println!("  ➕ {} lines added (recent)", git_info.lines_added.to_string().bright_green());
        println!("  ➖ {} lines deleted (recent)", git_info.lines_deleted.to_string().bright_red());
    }

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

        println!("\n▶️ {}", language.bright_white().bold());
        println!("  📄 {} files ({:.1}%)", lang_stats.files.to_string().bright_cyan(),
            (lang_stats.files as f64 / stats.total_files as f64 * 100.0).to_string().bright_white());
        println!("  📊 {} lines ({:.1}%)", total_lines.to_string().bright_green(),
            (total_lines as f64 / stats.total_lines as f64 * 100.0).to_string().bright_white());
        
        // Show composition percentages
        println!("  📈 {:.1}% code | {:.1}% comments | {:.1}% blank", 
            lang_stats.code_percentage.to_string().bright_green(),
            lang_stats.comment_percentage.to_string().bright_blue(),
            lang_stats.blank_percentage.to_string().bright_black());

        if args.complexity {
            println!("  🧮 {:.3} complexity score", lang_stats.complexity_score);
            println!("  🔧 {} functions | 🏗️ {} classes | 📦 {} imports", 
                lang_stats.functions.to_string().bright_yellow(),
                lang_stats.classes.to_string().bright_magenta(),
                lang_stats.imports.to_string().bright_cyan());

            if lang_stats.todos > 0 || lang_stats.fixmes > 0 {
                println!("  📝 {} TODOs | 🔧 {} FIXMEs", 
                    lang_stats.todos.to_string().bright_yellow(),
                    lang_stats.fixmes.to_string().bright_red());
            }
        }

        if args.size_stats {
            println!("  💾 {:.2} MB ({:.1} KB/file)", 
                lang_stats.total_size as f64 / 1_048_576.0,
                lang_stats.total_size as f64 / 1024.0 / lang_stats.files as f64);
        }

        if args.verbose {
            println!("  📏 {:.1} avg line length | {} max line length", 
                lang_stats.avg_line_length,
                lang_stats.max_line_length.to_string().bright_white());
        }
    }

    // Show top files if requested
    if let Some(ref metric) = args.top_files {
        show_top_files(stats, metric);
    }

    // Show hotspots if requested
    if args.hotspots && !stats.hotspots.is_empty() {
        println!("\n{} Code Hotspots (Risk Files)", "🔥".bright_red().bold());
        println!("{}", "-".repeat(80).bright_black());
        for (i, hotspot) in stats.hotspots.iter().enumerate() {
            println!("  {}. {} | {} lines | {:.3} complexity | {} TODOs | {:.1} KB", 
                (i + 1).to_string().bright_white(),
                hotspot.path.display().to_string().bright_red(),
                hotspot.lines.to_string().bright_white(),
                hotspot.complexity,
                hotspot.todos.to_string().bright_yellow(),
                (hotspot.size as f64 / 1024.0)
            );
        }
    }
}

fn main() {
    let args = Args::parse();

    if !args.path.exists() {
        eprintln!("❌ Path does not exist: {}", args.path.display());
        std::process::exit(1);
    }

    // Set thread count
    if args.threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.threads)
            .build_global()
            .unwrap();
    }

    println!("🚀 Initializing LOCO Advanced Analysis Engine...");
    println!("🎯 Target: {}", args.path.display().to_string().bright_white());

    let start_time = Instant::now();
    let files = collect_files_advanced(&args.path, &args);

    if files.is_empty() {
        println!("⚠️ No files found matching criteria.");
        return;
    }

    println!("⚙️ Processing {} files with {} threads...", 
        files.len().to_string().bright_white(),
        rayon::current_num_threads().to_string().bright_white());

    let languages: Arc<Mutex<HashMap<String, LanguageStats>>> = Arc::new(Mutex::new(HashMap::new()));
    let files_info: Arc<Mutex<Vec<FileInfo>>> = Arc::new(Mutex::new(Vec::new()));
    let creation_dates: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let modification_dates: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let processed = Arc::new(AtomicUsize::new(0));

    files.par_iter().for_each(|file_path| {
        if let Some(extension) = file_path.extension().and_then(|e| e.to_str()) {
            if let Some(config) = LanguageConfig::get_config(extension) {
                if let Some((file_stats, file_info)) = analyze_file_advanced(file_path, &config, &args) {
                    let language = get_language_name(extension);

                    // Update language stats
                    {
                        let mut languages_guard = languages.lock().unwrap();
                        let entry = languages_guard.entry(language)
                            .or_insert_with(LanguageStats::default);

                        entry.total_lines += file_stats.total_lines;
                        entry.code_lines += file_stats.code_lines;
                        entry.comment_lines += file_stats.comment_lines;
                        entry.blank_lines += file_stats.blank_lines;
                        entry.files += 1;
                        entry.total_size += file_stats.total_size;
                        entry.avg_line_length = (entry.avg_line_length * (entry.files - 1) as f64 + file_stats.avg_line_length) / entry.files as f64;
                        entry.max_line_length = entry.max_line_length.max(file_stats.max_line_length);
                        entry.complexity_score = (entry.complexity_score * (entry.files - 1) as f64 + file_stats.complexity_score) / entry.files as f64;
                        entry.functions += file_stats.functions;
                        entry.classes += file_stats.classes;
                        entry.imports += file_stats.imports;
                        entry.todos += file_stats.todos;
                        entry.fixmes += file_stats.fixmes;
                        
                        // Update percentages
                        if entry.total_lines > 0 {
                            entry.code_percentage = entry.code_lines as f64 / entry.total_lines as f64 * 100.0;
                            entry.comment_percentage = entry.comment_lines as f64 / entry.total_lines as f64 * 100.0;
                            entry.blank_percentage = entry.blank_lines as f64 / entry.total_lines as f64 * 100.0;
                        }
                    }

                    // Store file info
                    {
                        let mut files_info_guard = files_info.lock().unwrap();
                        files_info_guard.push(file_info.clone());
                    }

                    // Store timestamps if available
                    if args.time_analysis {
                        if let (Some(created), Some(modified)) = (file_info.created, file_info.modified) {
                            let mut creation_guard = creation_dates.lock().unwrap();
                            let mut modification_guard = modification_dates.lock().unwrap();
                            creation_guard.push(created);
                            modification_guard.push(modified);
                        }
                    }
                }
            }
        }

        let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
        if args.progress && current % 100 == 0 {
            print!("\r🔄 Progress: {}/{} files processed", current, files.len());
            io::stdout().flush().unwrap();
        }
    });

    if args.progress {
        println!("\r✅ Completed processing {} files!", files.len());
    }

    let final_languages = Arc::try_unwrap(languages).unwrap().into_inner().unwrap();
    let final_files_info = Arc::try_unwrap(files_info).unwrap().into_inner().unwrap();
    let final_creation_dates = Arc::try_unwrap(creation_dates).unwrap().into_inner().unwrap();
    let final_modification_dates = Arc::try_unwrap(modification_dates).unwrap().into_inner().unwrap();

    let analysis_time = start_time.elapsed().as_secs_f64();

    // Get git stats if requested
    let git_info = if args.git_stats {
        get_git_stats(&args.path)
    } else {
        None
    };

    // Detect hotspots if requested
    let hotspots = if args.hotspots {
        detect_hotspots(&final_files_info)
    } else {
        Vec::new()
    };

    let project_stats = ProjectStats {
        total_files: final_languages.values().map(|s| s.files).sum(),
        total_lines: final_languages.values().map(|s| s.total_lines).sum(),
        total_size: final_languages.values().map(|s| s.total_size).sum(),
        languages: final_languages,
        analysis_time,
        git_info,
        creation_dates: final_creation_dates,
        modification_dates: final_modification_dates,
        files_info: final_files_info,
        hotspots,
    };

    // Output results
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
        "html" => {
            let html = generate_html_report(&project_stats, &args);
            if let Some(output_path) = &args.output {
                fs::write(output_path, &html).unwrap();
                println!("HTML report saved to: {}", output_path.display());
            } else {
                println!("{}", html);
            }
        },
        _ => {
            print_advanced_results(&project_stats, &args);
        }
    }

    // Generate report if requested
    if args.report {
        let report_path = args.output.clone().unwrap_or_else(|| PathBuf::from("loco_report.html"));
        let html_report = generate_html_report(&project_stats, &args);
        fs::write(&report_path, &html_report).unwrap();
        println!("\n📊 Detailed HTML report saved to: {}", report_path.display().to_string().bright_green());
    }

    println!("\n✅ Analysis completed successfully! 🎉");
        }

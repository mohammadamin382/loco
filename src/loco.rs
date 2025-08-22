
use clap::Parser;
use colored::*;
use dashmap::DashMap;
use indicatif::{ProgressBar, ProgressStyle};
use memmap2::Mmap;
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Instant, UNIX_EPOCH};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(name = "loco")]
#[command(about = "🚀 Ultra-Fast Line Counter & Code Analyzer")]
#[command(version = "0.4.0")]
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

    /// Use memory mapping for large files
    #[arg(long)]
    use_mmap: bool,

    /// Enable caching for repeated analysis
    #[arg(long)]
    cache: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LanguageStats {
    total_lines: u64,
    code_lines: u64,
    comment_lines: u64,
    blank_lines: u64,
    files: u64,
    total_size: u64,
    avg_line_length: f64,
    max_line_length: usize,
    complexity_score: f64,
    functions: u64,
    classes: u64,
    imports: u64,
    todos: u64,
    fixmes: u64,
    code_percentage: f64,
    comment_percentage: f64,
    blank_percentage: f64,
    cyclomatic_complexity: f64,
    maintainability_index: f64,
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
            cyclomatic_complexity: 0.0,
            maintainability_index: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct FileInfo {
    path: PathBuf,
    language: String,
    lines: u64,
    size: u64,
    encoding: String,
    complexity: f64,
    created: Option<u64>,
    modified: Option<u64>,
    todos: u64,
    fixmes: u64,
    cyclomatic_complexity: f64,
    maintainability_index: f64,
    technical_debt_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
struct GitStats {
    total_commits: usize,
    contributors: usize,
    last_commit_date: Option<String>,
    lines_added: usize,
    lines_deleted: usize,
    branch: Option<String>,
    repository_age_days: Option<u64>,
    avg_commits_per_day: f64,
    most_active_author: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProjectStats {
    languages: HashMap<String, LanguageStats>,
    total_files: u64,
    total_lines: u64,
    total_size: u64,
    analysis_time: f64,
    git_info: Option<GitStats>,
    creation_dates: Vec<u64>,
    modification_dates: Vec<u64>,
    files_info: Vec<FileInfo>,
    hotspots: Vec<FileInfo>,
    directory_stats: HashMap<String, LanguageStats>,
    performance_metrics: PerformanceMetrics,
    quality_metrics: QualityMetrics,
}

#[derive(Debug, Clone, Serialize)]
struct PerformanceMetrics {
    files_per_second: f64,
    lines_per_second: f64,
    bytes_per_second: f64,
    peak_memory_usage: u64,
    cpu_utilization: f64,
}

#[derive(Debug, Clone, Serialize)]
struct QualityMetrics {
    overall_maintainability: f64,
    technical_debt_ratio: f64,
    test_coverage_estimate: f64,
    documentation_ratio: f64,
    code_duplication_ratio: f64,
}

#[derive(Clone)]
struct LanguageConfig {
    single_line_comments: Vec<String>,
    multi_line_comments: Vec<(String, String)>,
    function_keywords: Vec<String>,
    class_keywords: Vec<String>,
    import_keywords: Vec<String>,
    complexity_keywords: Vec<String>,
    test_keywords: Vec<String>,
    doc_keywords: Vec<String>,
}

impl LanguageConfig {
    fn get_config(extension: &str) -> Option<Self> {
        match extension.to_lowercase().as_str() {
            "rs" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["fn ".into(), "async fn ".into()],
                class_keywords: vec!["struct ".into(), "enum ".into(), "trait ".into(), "impl ".into()],
                import_keywords: vec!["use ".into(), "extern ".into(), "mod ".into()],
                complexity_keywords: vec!["if ".into(), "while ".into(), "for ".into(), "match ".into(), "loop ".into(), "else if ".into()],
                test_keywords: vec!["#[test]".into(), "#[cfg(test)]".into(), "assert!".into()],
                doc_keywords: vec!["///".into(), "//!".into(), "#[doc".into()],
            }),
            "py" | "pyw" | "pyi" => Some(Self {
                single_line_comments: vec!["#".into()],
                multi_line_comments: vec![("\"\"\"".into(), "\"\"\"".into()), ("'''".into(), "'''".into())],
                function_keywords: vec!["def ".into(), "async def ".into(), "lambda ".into()],
                class_keywords: vec!["class ".into()],
                import_keywords: vec!["import ".into(), "from ".into()],
                complexity_keywords: vec!["if ".into(), "while ".into(), "for ".into(), "try ".into(), "except ".into(), "with ".into(), "elif ".into()],
                test_keywords: vec!["def test_".into(), "import unittest".into(), "import pytest".into()],
                doc_keywords: vec!["\"\"\"".into(), "'''".into(), "# TODO".into(), "# FIXME".into()],
            }),
            "js" | "ts" | "jsx" | "tsx" | "mjs" | "cjs" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["function ".into(), "=>".into(), "async ".into(), "const ".into(), "let ".into(), "var ".into()],
                class_keywords: vec!["class ".into(), "interface ".into(), "type ".into(), "enum ".into()],
                import_keywords: vec!["import ".into(), "require(".into(), "export ".into(), "from ".into()],
                complexity_keywords: vec!["if ".into(), "while ".into(), "for ".into(), "switch ".into(), "try ".into(), "catch ".into(), "else if ".into()],
                test_keywords: vec!["describe(".into(), "it(".into(), "test(".into(), "expect(".into()],
                doc_keywords: vec!["/**".into(), "//".into(), "@param".into(), "@return".into()],
            }),
            "java" | "kt" | "scala" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["public ".into(), "private ".into(), "protected ".into(), "static ".into()],
                class_keywords: vec!["class ".into(), "interface ".into(), "enum ".into(), "abstract ".into()],
                import_keywords: vec!["import ".into(), "package ".into()],
                complexity_keywords: vec!["if ".into(), "while ".into(), "for ".into(), "switch ".into(), "try ".into(), "catch ".into(), "else if ".into()],
                test_keywords: vec!["@Test".into(), "junit".into(), "testng".into()],
                doc_keywords: vec!["/**".into(), "//".into(), "@param".into(), "@return".into()],
            }),
            "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "c++" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["int ".into(), "void ".into(), "char ".into(), "float ".into(), "double ".into(), "bool ".into()],
                class_keywords: vec!["class ".into(), "struct ".into(), "union ".into(), "enum ".into(), "namespace ".into()],
                import_keywords: vec!["#include".into(), "#import".into(), "using ".into()],
                complexity_keywords: vec!["if ".into(), "while ".into(), "for ".into(), "switch ".into(), "else if ".into()],
                test_keywords: vec!["TEST(".into(), "ASSERT_".into(), "EXPECT_".into()],
                doc_keywords: vec!["/**".into(), "//!".into(), "///".into()],
            }),
            "go" => Some(Self {
                single_line_comments: vec!["//".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["func ".into()],
                class_keywords: vec!["type ".into(), "struct ".into(), "interface ".into()],
                import_keywords: vec!["import ".into(), "package ".into()],
                complexity_keywords: vec!["if ".into(), "for ".into(), "switch ".into(), "select ".into(), "else if ".into()],
                test_keywords: vec!["func Test".into(), "testing.T".into()],
                doc_keywords: vec!["//".into(), "/*".into()],
            }),
            "php" => Some(Self {
                single_line_comments: vec!["//".into(), "#".into()],
                multi_line_comments: vec![("/*".into(), "*/".into())],
                function_keywords: vec!["function ".into(), "public function ".into(), "private function ".into()],
                class_keywords: vec!["class ".into(), "interface ".into(), "trait ".into(), "abstract ".into()],
                import_keywords: vec!["require".into(), "include".into(), "use ".into()],
                complexity_keywords: vec!["if ".into(), "while ".into(), "for ".into(), "switch ".into(), "try ".into(), "catch ".into()],
                test_keywords: vec!["function test".into(), "PHPUnit".into()],
                doc_keywords: vec!["/**".into(), "//".into(), "*".into()],
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
        "kt" => "Kotlin 🟪".to_string(),
        "scala" => "Scala 🔴".to_string(),
        "c" => "C 🔧".to_string(),
        "cpp" | "cc" | "cxx" | "c++" => "C++ ⚡".to_string(),
        "h" => "C Header 📋".to_string(),
        "hpp" | "hxx" => "C++ Header 📋".to_string(),
        "go" => "Go 🐹".to_string(),
        "php" => "PHP 🐘".to_string(),
        "rb" => "Ruby 💎".to_string(),
        "swift" => "Swift 🦉".to_string(),
        "html" | "htm" => "HTML 🌐".to_string(),
        "css" | "scss" | "sass" => "CSS 🎨".to_string(),
        "json" => "JSON 📊".to_string(),
        "yaml" | "yml" => "YAML 📝".to_string(),
        "toml" => "TOML ⚙️".to_string(),
        "xml" => "XML 📄".to_string(),
        "md" | "markdown" => "Markdown 📖".to_string(),
        "sh" | "bash" | "zsh" => "Shell 🐚".to_string(),
        "sql" => "SQL 🗃️".to_string(),
        "r" => "R 📈".to_string(),
        "m" => "MATLAB 🧮".to_string(),
        _ => format!("Unknown ({})", extension),
    }
}

fn detect_encoding_advanced(file_path: &Path) -> String {
    match fs::read(file_path) {
        Ok(bytes) => {
            if bytes.is_empty() {
                return "Empty".to_string();
            }

            // Check BOM
            if bytes.len() >= 3 && &bytes[0..3] == b"\xEF\xBB\xBF" {
                return "UTF-8 BOM".to_string();
            }
            if bytes.len() >= 2 {
                if &bytes[0..2] == b"\xFF\xFE" {
                    return "UTF-16 LE".to_string();
                }
                if &bytes[0..2] == b"\xFE\xFF" {
                    return "UTF-16 BE".to_string();
                }
            }

            // Sample analysis for performance
            let sample_size = std::cmp::min(1024, bytes.len());
            let sample = &bytes[0..sample_size];

            let ascii_count = sample.iter().filter(|&&b| b.is_ascii()).count();
            let ascii_ratio = ascii_count as f64 / sample.len() as f64;

            if ascii_ratio == 1.0 {
                "ASCII".to_string()
            } else if std::str::from_utf8(sample).is_ok() {
                "UTF-8".to_string()
            } else if sample.iter().any(|&b| b > 127) {
                "Binary/Unknown".to_string()
            } else {
                "ISO-8859-1".to_string()
            }
        },
        Err(_) => "Unreadable".to_string(),
    }
}

fn get_file_times(file_path: &Path) -> (Option<u64>, Option<u64>) {
    fs::metadata(file_path).ok().map_or((None, None), |metadata| {
        let created = metadata.created().ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        let modified = metadata.modified().ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        (created, modified)
    })
}

fn get_git_stats(path: &Path) -> Option<GitStats> {
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
        repository_age_days: None,
        avg_commits_per_day: 0.0,
        most_active_author: None,
    };

    // Get total commits
    if let Ok(output) = std::process::Command::new("git")
        .args(&["rev-list", "--count", "HEAD"])
        .current_dir(path)
        .output()
    {
        if let Ok(count_str) = String::from_utf8(output.stdout) {
            git_stats.total_commits = count_str.trim().parse().unwrap_or(0);
        }
    }

    // Get contributors count and most active author
    if let Ok(output) = std::process::Command::new("git")
        .args(&["shortlog", "-sn"])
        .current_dir(path)
        .output()
    {
        if let Ok(contributors_str) = String::from_utf8(output.stdout) {
            let lines: Vec<&str> = contributors_str.lines().collect();
            git_stats.contributors = lines.len();
            if let Some(first_line) = lines.first() {
                if let Some(author) = first_line.split_whitespace().skip(1).next() {
                    git_stats.most_active_author = Some(author.to_string());
                }
            }
        }
    }

    // Get last commit date and calculate repository age
    if let Ok(output) = std::process::Command::new("git")
        .args(&["log", "-1", "--format=%cd", "--date=short"])
        .current_dir(path)
        .output()
    {
        if let Ok(date_str) = String::from_utf8(output.stdout) {
            git_stats.last_commit_date = Some(date_str.trim().to_string());
        }
    }

    // Get first commit date for age calculation
    if let Ok(output) = std::process::Command::new("git")
        .args(&["log", "--reverse", "--format=%ct", "-1"])
        .current_dir(path)
        .output()
    {
        if let Ok(timestamp_str) = String::from_utf8(output.stdout) {
            if let Ok(first_commit_timestamp) = timestamp_str.trim().parse::<u64>() {
                let now = std::time::SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let age_days = (now - first_commit_timestamp) / (24 * 3600);
                git_stats.repository_age_days = Some(age_days);
                
                if age_days > 0 {
                    git_stats.avg_commits_per_day = git_stats.total_commits as f64 / age_days as f64;
                }
            }
        }
    }

    // Get current branch
    if let Ok(output) = std::process::Command::new("git")
        .args(&["branch", "--show-current"])
        .current_dir(path)
        .output()
    {
        if let Ok(branch_str) = String::from_utf8(output.stdout) {
            git_stats.branch = Some(branch_str.trim().to_string());
        }
    }

    // Get lines added/deleted statistics
    if let Ok(output) = std::process::Command::new("git")
        .args(&["log", "--numstat", "--pretty=format:", "--since=1.year.ago"])
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
    let metadata = fs::metadata(file_path).ok()?;
    let file_size = metadata.len();

    // Use memory mapping for large files if enabled
    let content = if args.use_mmap && file_size > 1024 * 1024 {
        let file = File::open(file_path).ok()?;
        let mmap = unsafe { Mmap::map(&file).ok()? };
        std::str::from_utf8(&mmap).ok()?.to_string()
    } else {
        fs::read_to_string(file_path).ok()?
    };

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len() as u64;

    let mut code_lines = 0u64;
    let mut comment_lines = 0u64;
    let mut blank_lines = 0u64;
    let mut functions = 0u64;
    let mut classes = 0u64;
    let mut imports = 0u64;
    let mut todos = 0u64;
    let mut fixmes = 0u64;
    let mut complexity_score = 0.0;
    let mut cyclomatic_complexity = 0.0;
    let mut max_line_length = 0;
    let mut total_chars = 0;
    let mut _test_indicators = 0u64;
    let mut _doc_indicators = 0u64;

    let mut in_multi_comment = false;
    let mut multi_comment_end = String::new();
    let mut nesting_level = 0;

    for line in &lines {
        let trimmed = line.trim();
        let line_length = line.len();
        max_line_length = max_line_length.max(line_length);
        total_chars += line_length;

        if trimmed.is_empty() {
            blank_lines += 1;
            continue;
        }

        // Enhanced pattern detection
        let line_upper = trimmed.to_uppercase();
        if line_upper.contains("TODO") { todos += 1; }
        if line_upper.contains("FIXME") { fixmes += 1; }

        // Test detection
        for test_keyword in &config.test_keywords {
            if trimmed.contains(test_keyword) {
                _test_indicators += 1;
                break;
            }
        }

        // Documentation detection
        for doc_keyword in &config.doc_keywords {
            if trimmed.contains(doc_keyword) {
                _doc_indicators += 1;
                break;
            }
        }

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
                let before_comment = line_content[..start_pos].trim();

                if let Some(end_pos) = line_content[start_pos + start.len()..].find(end) {
                    let after_comment = line_content[start_pos + start.len() + end_pos + end.len()..].trim();
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
                    let before_comment = line_content[..pos].trim();
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

                // Enhanced complexity analysis
                for keyword in &config.complexity_keywords {
                    if line_content.contains(keyword) {
                        complexity_score += 1.0;
                        cyclomatic_complexity += 1.0;
                    }
                }

                // Nesting level tracking
                let open_braces = line_content.matches('{').count();
                let close_braces = line_content.matches('}').count();
                nesting_level += open_braces as i32 - close_braces as i32;
                if nesting_level > 0 {
                    complexity_score += 0.1;
                }

                // Pattern analysis
                for keyword in &config.function_keywords {
                    if line_content.contains(keyword) { functions += 1; break; }
                }
                for keyword in &config.class_keywords {
                    if line_content.contains(keyword) { classes += 1; break; }
                }
                for keyword in &config.import_keywords {
                    if line_content.contains(keyword) { imports += 1; break; }
                }
            }
        }
    }

    let avg_line_length = if total_lines > 0 {
        total_chars as f64 / total_lines as f64
    } else { 0.0 };

    // Enhanced complexity calculations
    complexity_score = if code_lines > 0 {
        complexity_score / code_lines as f64
    } else { 0.0 };

    cyclomatic_complexity = if functions > 0 {
        (cyclomatic_complexity + functions as f64) / functions as f64
    } else { 1.0 };

    // Calculate maintainability index (simplified version)
    let maintainability_index = if code_lines > 0 {
        let volume = (total_lines as f64).ln() * 2.0;
        let complexity_factor = cyclomatic_complexity.ln();
        let comment_ratio = comment_lines as f64 / total_lines as f64;
        
        171.0 - 5.2 * volume - 0.23 * complexity_factor - 16.2 * (1.0 - comment_ratio).ln()
    } else { 0.0 };

    // Technical debt ratio
    let technical_debt_ratio = (todos + fixmes) as f64 / total_lines as f64 * 100.0;

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
        total_size: file_size,
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
        cyclomatic_complexity,
        maintainability_index,
    };

    let file_info = FileInfo {
        path: file_path.to_path_buf(),
        language,
        lines: total_lines,
        size: file_size,
        encoding,
        complexity: complexity_score,
        created,
        modified,
        todos,
        fixmes,
        cyclomatic_complexity,
        maintainability_index,
        technical_debt_ratio,
    };

    Some((lang_stats, file_info))
}

fn collect_files_parallel(path: &Path, args: &Args) -> Vec<PathBuf> {
    let exclude_regex = args.exclude.as_ref()
        .and_then(|exclude| Regex::new(exclude).ok());

    let include_exts: Option<Vec<String>> = args.include.as_ref().map(|s| 
        s.split(',').map(|ext| ext.trim().to_lowercase()).collect()
    );

    let default_excludes = [
        "target", "node_modules", ".git", "build", "dist", "__pycache__", 
        ".cargo", ".next", ".nuxt", "vendor", "coverage", ".pytest_cache",
        ".vscode", ".idea", "bin", "obj", ".vs", "packages", ".svn", ".hg",
        "deps", "tmp", "temp", "cache", ".cache", "logs", ".terraform",
        "venv", "env", ".env", "bower_components", ".gradle"
    ];

    let max_size_bytes = args.max_size * 1024 * 1024;

    WalkDir::new(path)
        .into_iter()
        .par_bridge()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let file_path = entry.path();
            
            // Quick size check
            if let Ok(metadata) = file_path.metadata() {
                if metadata.len() > max_size_bytes {
                    return None;
                }
            }

            let path_str = file_path.to_string_lossy();

            // Regex exclude check
            if let Some(ref regex) = exclude_regex {
                if regex.is_match(&path_str) {
                    return None;
                }
            }

            // Default excludes check
            for exclude in &default_excludes {
                if path_str.contains(&format!("/{}/", exclude)) || 
                   path_str.contains(&format!("\\{}\\", exclude)) {
                    return None;
                }
            }

            // Extension filter
            if let Some(ref include_exts) = include_exts {
                if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                    if !include_exts.contains(&ext.to_lowercase()) {
                        return None;
                    }
                } else {
                    return None;
                }
            } else {
                // Only include files with known extensions
                if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                    if LanguageConfig::get_config(ext).is_none() {
                        return None;
                    }
                } else {
                    return None;
                }
            }

            Some(file_path.to_path_buf())
        })
        .collect()
}

fn detect_hotspots(files_info: &[FileInfo]) -> Vec<FileInfo> {
    if files_info.is_empty() {
        return Vec::new();
    }

    // Calculate dynamic thresholds based on dataset
    let avg_lines: f64 = files_info.iter().map(|f| f.lines as f64).sum::<f64>() / files_info.len() as f64;
    let avg_complexity: f64 = files_info.iter().map(|f| f.complexity).sum::<f64>() / files_info.len() as f64;
    let avg_todos: f64 = files_info.iter().map(|f| f.todos as f64).sum::<f64>() / files_info.len() as f64;

    let large_file_threshold = (avg_lines * 2.0) as u64;
    let high_complexity_threshold = avg_complexity * 1.5;
    let high_todos_threshold = (avg_todos * 2.0) as u64;
    let large_size_threshold = 100 * 1024; // 100KB

    let mut hotspots: Vec<FileInfo> = files_info.iter()
        .filter_map(|file| {
            let mut risk_score = 0;

            if file.lines > large_file_threshold { risk_score += 2; }
            if file.complexity > high_complexity_threshold { risk_score += 3; }
            if file.todos > high_todos_threshold { risk_score += 2; }
            if file.size > large_size_threshold { risk_score += 1; }
            if file.maintainability_index < 20.0 { risk_score += 2; }
            if file.technical_debt_ratio > 5.0 { risk_score += 1; }

            if risk_score >= 3 {
                Some(file.clone())
            } else {
                None
            }
        })
        .collect();

    // Sort by comprehensive risk score
    hotspots.sort_by(|a, b| {
        let risk_a = a.complexity + (a.size as f64 / 1000.0) + (a.todos as f64 / 10.0) 
                     - (a.maintainability_index / 10.0) + a.technical_debt_ratio;
        let risk_b = b.complexity + (b.size as f64 / 1000.0) + (b.todos as f64 / 10.0) 
                     - (b.maintainability_index / 10.0) + b.technical_debt_ratio;
        risk_b.partial_cmp(&risk_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    hotspots.truncate(15); // Top 15 hotspots
    hotspots
}

fn generate_html_report(stats: &ProjectStats, _args: &Args) -> String {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    
    format!(r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>LOCO Advanced Code Analysis Report</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{ 
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; 
            line-height: 1.6; 
            color: #333; 
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            padding: 20px;
        }}
        .container {{ 
            max-width: 1400px; 
            margin: 0 auto; 
            background: rgba(255, 255, 255, 0.95); 
            border-radius: 20px; 
            box-shadow: 0 20px 40px rgba(0,0,0,0.1);
            backdrop-filter: blur(10px);
            overflow: hidden;
        }}
        .header {{ 
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white; 
            padding: 40px 20px; 
            text-align: center; 
        }}
        .header h1 {{ font-size: 3em; margin-bottom: 10px; text-shadow: 2px 2px 4px rgba(0,0,0,0.3); }}
        .header p {{ font-size: 1.2em; opacity: 0.9; }}
        .content {{ padding: 40px; }}
        .stats-grid {{ 
            display: grid; 
            grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); 
            gap: 25px; 
            margin: 30px 0; 
        }}
        .stat-card {{ 
            background: linear-gradient(145deg, #f0f0f0, #ffffff);
            padding: 25px; 
            border-radius: 15px; 
            text-align: center; 
            box-shadow: 0 8px 16px rgba(0,0,0,0.1);
            transition: transform 0.3s, box-shadow 0.3s;
        }}
        .stat-card:hover {{
            transform: translateY(-5px);
            box-shadow: 0 12px 24px rgba(0,0,0,0.15);
        }}
        .stat-value {{ 
            font-size: 2.5em; 
            font-weight: bold; 
            background: linear-gradient(135deg, #667eea, #764ba2);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            margin-bottom: 10px;
        }}
        .stat-label {{ font-size: 1.1em; color: #666; font-weight: 500; }}
        .section {{ margin: 40px 0; }}
        .section h2 {{ 
            color: #333; 
            font-size: 2em; 
            margin-bottom: 20px; 
            padding-bottom: 10px;
            border-bottom: 3px solid #667eea;
        }}
        .language-table {{ 
            width: 100%; 
            border-collapse: collapse; 
            margin: 20px 0;
            border-radius: 10px;
            overflow: hidden;
            box-shadow: 0 4px 8px rgba(0,0,0,0.1);
        }}
        .language-table th {{ 
            background: linear-gradient(135deg, #667eea, #764ba2);
            color: white; 
            padding: 15px; 
            font-weight: 600;
            text-align: left;
        }}
        .language-table td {{ 
            padding: 12px 15px; 
            border-bottom: 1px solid #eee;
        }}
        .language-table tr:nth-child(even) {{ background-color: #f8f9fa; }}
        .language-table tr:hover {{ background-color: #e3f2fd; }}
        .progress-bar {{ 
            background: #e0e0e0; 
            border-radius: 10px; 
            overflow: hidden; 
            height: 8px; 
            margin: 5px 0; 
        }}
        .progress-fill {{ 
            height: 100%; 
            background: linear-gradient(90deg, #667eea, #764ba2); 
            transition: width 0.3s ease;
        }}
        .hotspot {{ 
            background: linear-gradient(135deg, #ff6b6b, #ee5a52);
            color: white; 
            padding: 15px; 
            margin: 10px 0; 
            border-radius: 10px; 
            box-shadow: 0 4px 8px rgba(255,107,107,0.3);
        }}
        .quality-metrics {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 20px;
            margin: 20px 0;
        }}
        .metric-card {{
            background: #f8f9fa;
            padding: 20px;
            border-radius: 10px;
            border-left: 4px solid #667eea;
        }}
        .chart-container {{
            background: white;
            padding: 20px;
            border-radius: 10px;
            margin: 20px 0;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        @media (max-width: 768px) {{
            .header h1 {{ font-size: 2em; }}
            .content {{ padding: 20px; }}
            .stats-grid {{ grid-template-columns: 1fr; }}
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🚀 LOCO Advanced Analysis</h1>
            <p>Ultra-Fast Code Intelligence Report • Generated {}</p>
        </div>

        <div class="content">
            <div class="stats-grid">
                <div class="stat-card">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">📁 Total Files</div>
                </div>
                <div class="stat-card">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">📏 Total Lines</div>
                </div>
                <div class="stat-card">
                    <div class="stat-value">{:.2}</div>
                    <div class="stat-label">💾 Size (MB)</div>
                </div>
                <div class="stat-card">
                    <div class="stat-value">{:.3}</div>
                    <div class="stat-label">⚡ Analysis Time (s)</div>
                </div>
            </div>

            <div class="section">
                <h2>📊 Language Statistics</h2>
                <table class="language-table">
                    <thead>
                        <tr>
                            <th>Language</th>
                            <th>Files</th>
                            <th>Lines</th>
                            <th>Code %</th>
                            <th>Comments %</th>
                            <th>Complexity</th>
                            <th>Maintainability</th>
                        </tr>
                    </thead>
                    <tbody>
                        {}
                    </tbody>
                </table>
            </div>

            <div class="section">
                <h2>📈 Quality Metrics</h2>
                <div class="quality-metrics">
                    <div class="metric-card">
                        <h3>Overall Maintainability</h3>
                        <div class="stat-value" style="font-size: 1.5em;">{:.1}</div>
                    </div>
                    <div class="metric-card">
                        <h3>Technical Debt Ratio</h3>
                        <div class="stat-value" style="font-size: 1.5em;">{:.2}%</div>
                    </div>
                    <div class="metric-card">
                        <h3>Documentation Ratio</h3>
                        <div class="stat-value" style="font-size: 1.5em;">{:.2}%</div>
                    </div>
                </div>
            </div>

            {}

            {}
        </div>
    </div>
</body>
</html>
"#,
        timestamp,
        stats.total_files,
        stats.total_lines,
        stats.total_size as f64 / 1_048_576.0,
        stats.analysis_time,
        generate_language_rows_enhanced(stats),
        stats.quality_metrics.overall_maintainability,
        stats.quality_metrics.technical_debt_ratio,
        stats.quality_metrics.documentation_ratio,
        generate_git_section_enhanced(stats),
        generate_hotspots_section_enhanced(stats)
    )
}

fn generate_language_rows_enhanced(stats: &ProjectStats) -> String {
    let mut rows = String::new();
    let mut sorted_languages: Vec<_> = stats.languages.iter().collect();
    sorted_languages.sort_by(|a, b| b.1.total_lines.cmp(&a.1.total_lines));

    for (language, lang_stats) in sorted_languages.iter().take(10) {
        rows.push_str(&format!(
            r#"<tr>
                <td><strong>{}</strong></td>
                <td>{}</td>
                <td>{}</td>
                <td>{:.1}%</td>
                <td>{:.1}%</td>
                <td>{:.3}</td>
                <td>{:.1}</td>
            </tr>"#,
            language,
            lang_stats.files,
            lang_stats.total_lines,
            lang_stats.code_percentage,
            lang_stats.comment_percentage,
            lang_stats.complexity_score,
            lang_stats.maintainability_index
        ));
    }
    rows
}

fn generate_git_section_enhanced(stats: &ProjectStats) -> String {
    if let Some(ref git_info) = stats.git_info {
        format!(r#"
        <div class="section">
            <h2>🔄 Git Repository Analytics</h2>
            <div class="stats-grid">
                <div class="stat-card">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">📊 Total Commits</div>
                </div>
                <div class="stat-card">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">👥 Contributors</div>
                </div>
                <div class="stat-card">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">➕ Lines Added</div>
                </div>
                <div class="stat-card">
                    <div class="stat-value">{:.1}</div>
                    <div class="stat-label">📈 Commits/Day</div>
                </div>
            </div>
            <div style="margin-top: 20px;">
                <p><strong>🌿 Current Branch:</strong> {}</p>
                <p><strong>📅 Last Commit:</strong> {}</p>
                <p><strong>🏆 Most Active:</strong> {}</p>
                {}
            </div>
        </div>
        "#,
        git_info.total_commits,
        git_info.contributors,
        git_info.lines_added,
        git_info.avg_commits_per_day,
        git_info.branch.as_ref().unwrap_or(&"Unknown".to_string()),
        git_info.last_commit_date.as_ref().unwrap_or(&"Unknown".to_string()),
        git_info.most_active_author.as_ref().unwrap_or(&"Unknown".to_string()),
        if let Some(age_days) = git_info.repository_age_days {
            format!("<p><strong>📆 Repository Age:</strong> {} days</p>", age_days)
        } else {
            String::new()
        }
        )
    } else {
        String::new()
    }
}

fn generate_hotspots_section_enhanced(stats: &ProjectStats) -> String {
    if !stats.hotspots.is_empty() {
        let mut section = String::from(r#"
        <div class="section">
            <h2>🔥 Code Hotspots & Risk Analysis</h2>
            <p style="margin-bottom: 20px; color: #666;">Files that may need attention based on complexity, size, and technical debt indicators.</p>
        "#);

        for (i, hotspot) in stats.hotspots.iter().enumerate() {
            section.push_str(&format!(
                r#"<div class="hotspot">
                    <div style="display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap;">
                        <div style="flex: 1; min-width: 200px;">
                            <strong>#{} {}</strong><br>
                            <small style="opacity: 0.9;">{}</small>
                        </div>
                        <div style="text-align: right;">
                            📏 {} lines | 🧮 {:.3} complexity<br>
                            📝 {} TODOs | 💾 {:.1} KB | 🔧 {:.1} MI
                        </div>
                    </div>
                </div>"#,
                i + 1,
                hotspot.path.file_name().unwrap_or_default().to_string_lossy(),
                hotspot.path.display(),
                hotspot.lines,
                hotspot.complexity,
                hotspot.todos,
                hotspot.size as f64 / 1024.0,
                hotspot.maintainability_index
            ));
        }
        
        section.push_str("</div>");
        section
    } else {
        r#"
        <div class="section">
            <h2>✅ Code Quality Status</h2>
            <div style="background: linear-gradient(135deg, #4CAF50, #45a049); color: white; padding: 20px; border-radius: 10px; text-align: center;">
                <h3>Excellent! No significant hotspots detected.</h3>
                <p>Your codebase appears to be well-maintained with good quality metrics.</p>
            </div>
        </div>
        "#.to_string()
    }
}

fn show_top_files_enhanced(stats: &ProjectStats, metric: &str) {
    let mut files = stats.files_info.clone();

    match metric {
        "lines" => files.sort_by(|a, b| b.lines.cmp(&a.lines)),
        "complexity" => files.sort_by(|a, b| b.complexity.partial_cmp(&a.complexity).unwrap_or(std::cmp::Ordering::Equal)),
        "todos" => files.sort_by(|a, b| b.todos.cmp(&a.todos)),
        "size" => files.sort_by(|a, b| b.size.cmp(&a.size)),
        "maintainability" => files.sort_by(|a, b| a.maintainability_index.partial_cmp(&b.maintainability_index).unwrap_or(std::cmp::Ordering::Equal)),
        "debt" => files.sort_by(|a, b| b.technical_debt_ratio.partial_cmp(&a.technical_debt_ratio).unwrap_or(std::cmp::Ordering::Equal)),
        _ => return,
    }

    files.truncate(10);

    println!("\n{} Top 10 Files by {}", "🏆".bright_yellow().bold(), metric.to_uppercase());
    println!("{}", "─".repeat(90).bright_black());

    for (i, file) in files.iter().enumerate() {
        let value = match metric {
            "lines" => format!("{} lines", file.lines),
            "complexity" => format!("{:.3}", file.complexity),
            "todos" => format!("{} todos", file.todos),
            "size" => format!("{:.1} KB", file.size as f64 / 1024.0),
            "maintainability" => format!("{:.1} MI", file.maintainability_index),
            "debt" => format!("{:.2}% debt", file.technical_debt_ratio),
            _ => "0".to_string(),
        };

        let indicator = match i {
            0 => "🥇",
            1 => "🥈", 
            2 => "🥉",
            _ => "📄",
        };

        println!("  {} {}. {} | {}", 
            indicator,
            (i + 1).to_string().bright_white(),
            file.path.display().to_string().bright_cyan(),
            value.bright_green()
        );
    }
}

fn calculate_quality_metrics(stats: &ProjectStats) -> QualityMetrics {
    let total_files = stats.total_files as f64;
    let total_lines = stats.total_lines as f64;

    if total_files == 0.0 || total_lines == 0.0 {
        return QualityMetrics {
            overall_maintainability: 0.0,
            technical_debt_ratio: 0.0,
            test_coverage_estimate: 0.0,
            documentation_ratio: 0.0,
            code_duplication_ratio: 0.0,
        };
    }

    // Calculate weighted maintainability
    let overall_maintainability = stats.languages.values()
        .map(|lang| lang.maintainability_index * lang.total_lines as f64)
        .sum::<f64>() / total_lines;

    // Calculate technical debt ratio
    let total_todos = stats.languages.values().map(|lang| lang.todos).sum::<u64>();
    let total_fixmes = stats.languages.values().map(|lang| lang.fixmes).sum::<u64>();
    let technical_debt_ratio = (total_todos + total_fixmes) as f64 / total_lines * 100.0;

    // Estimate test coverage based on test files and test keywords
    let test_file_count = stats.files_info.iter()
        .filter(|file| {
            let path_str = file.path.to_string_lossy().to_lowercase();
            path_str.contains("test") || path_str.contains("spec")
        })
        .count() as f64;
    let test_coverage_estimate = (test_file_count / total_files * 100.0).min(100.0);

    // Calculate documentation ratio
    let total_comments = stats.languages.values().map(|lang| lang.comment_lines).sum::<u64>();
    let documentation_ratio = total_comments as f64 / total_lines * 100.0;

    // Estimate code duplication (simplified - based on average line length variance)
    let avg_line_lengths: Vec<f64> = stats.languages.values()
        .map(|lang| lang.avg_line_length)
        .collect();
    let mean_avg_length = avg_line_lengths.iter().sum::<f64>() / avg_line_lengths.len() as f64;
    let variance = avg_line_lengths.iter()
        .map(|&x| (x - mean_avg_length).powi(2))
        .sum::<f64>() / avg_line_lengths.len() as f64;
    let code_duplication_ratio = (variance / mean_avg_length * 10.0).min(30.0);

    QualityMetrics {
        overall_maintainability,
        technical_debt_ratio,
        test_coverage_estimate,
        documentation_ratio,
        code_duplication_ratio,
    }
}

fn print_advanced_results(stats: &ProjectStats, args: &Args) {
    println!("{}", "🚀 LOCO - Ultra-Fast Code Intelligence".bright_cyan().bold());
    println!("{}", "═".repeat(80).bright_black());

    println!("\n{} Project Overview", "📊".bright_magenta().bold());
    println!("  📁 {} files analyzed", stats.total_files.to_string().bright_white());
    println!("  📏 {} total lines of code", stats.total_lines.to_string().bright_white());
    println!("  💾 {:.2} MB total size", (stats.total_size as f64 / 1_048_576.0).to_string().bright_white());
    
    // Performance metrics
    println!("\n{} Performance Metrics", "⚡".bright_yellow().bold());
    println!("  ⏱️  {:.3}s analysis time", stats.analysis_time.to_string().bright_white());
    println!("  🚀 {:.1} files/sec", stats.performance_metrics.files_per_second.to_string().bright_cyan());
    println!("  📈 {:.0} lines/sec", stats.performance_metrics.lines_per_second.to_string().bright_cyan());
    println!("  💽 {:.1} MB/sec", (stats.performance_metrics.bytes_per_second / 1_048_576.0).to_string().bright_cyan());

    // Quality metrics
    println!("\n{} Quality Assessment", "🎯".bright_green().bold());
    println!("  🔧 {:.1} overall maintainability", stats.quality_metrics.overall_maintainability.to_string().bright_white());
    println!("  ⚠️  {:.2}% technical debt ratio", stats.quality_metrics.technical_debt_ratio.to_string().bright_yellow());
    println!("  📊 {:.1}% estimated test coverage", stats.quality_metrics.test_coverage_estimate.to_string().bright_blue());
    println!("  📖 {:.1}% documentation ratio", stats.quality_metrics.documentation_ratio.to_string().bright_green());

    // Git statistics
    if let Some(ref git_info) = stats.git_info {
        println!("\n{} Git Repository Intelligence", "🔄".bright_blue().bold());
        println!("  📊 {} total commits", git_info.total_commits.to_string().bright_white());
        println!("  👥 {} contributors", git_info.contributors.to_string().bright_white());
        if let Some(ref last_commit) = git_info.last_commit_date {
            println!("  📅 Last commit: {}", last_commit.bright_white());
        }
        if let Some(ref branch) = git_info.branch {
            println!("  🌿 Current branch: {}", branch.bright_white());
        }
        if let Some(ref author) = git_info.most_active_author {
            println!("  🏆 Most active: {}", author.bright_white());
        }
        if let Some(age_days) = git_info.repository_age_days {
            println!("  📆 Repository age: {} days", age_days.to_string().bright_white());
        }
        println!("  📈 {:.2} commits/day average", git_info.avg_commits_per_day.to_string().bright_cyan());
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

    println!("\n{} Language Intelligence", "🔤".bright_blue().bold());
    println!("{}", "─".repeat(90).bright_black());

    for (language, lang_stats) in &sorted_languages {
        let total_lines = lang_stats.total_lines;
        if total_lines < args.min_lines as u64 { continue; }

        println!("\n▶️ {}", language.bright_white().bold());
        
        // Basic stats
        println!("  📄 {} files ({:.1}%)", 
            lang_stats.files.to_string().bright_cyan(),
            (lang_stats.files as f64 / stats.total_files as f64 * 100.0).to_string().bright_white()
        );
        println!("  📊 {} lines ({:.1}%)", 
            total_lines.to_string().bright_green(),
            (total_lines as f64 / stats.total_lines as f64 * 100.0).to_string().bright_white()
        );
        
        // Code composition
        println!("  📈 {:.1}% code | {:.1}% comments | {:.1}% blank", 
            lang_stats.code_percentage.to_string().bright_green(),
            lang_stats.comment_percentage.to_string().bright_blue(),
            lang_stats.blank_percentage.to_string().bright_black()
        );

        if args.complexity {
            println!("  🧮 {:.3} avg complexity | {:.3} cyclomatic complexity", 
                lang_stats.complexity_score,
                lang_stats.cyclomatic_complexity
            );
            println!("  🔧 {} functions | 🏗️ {} classes | 📦 {} imports", 
                lang_stats.functions.to_string().bright_yellow(),
                lang_stats.classes.to_string().bright_magenta(),
                lang_stats.imports.to_string().bright_cyan()
            );
            println!("  🔧 {:.1} maintainability index", 
                lang_stats.maintainability_index.to_string().bright_blue()
            );

            if lang_stats.todos > 0 || lang_stats.fixmes > 0 {
                println!("  📝 {} TODOs | 🔧 {} FIXMEs", 
                    lang_stats.todos.to_string().bright_yellow(),
                    lang_stats.fixmes.to_string().bright_red()
                );
            }
        }

        if args.size_stats {
            println!("  💾 {:.2} MB ({:.1} KB/file)", 
                lang_stats.total_size as f64 / 1_048_576.0,
                lang_stats.total_size as f64 / 1024.0 / lang_stats.files as f64
            );
        }

        if args.verbose {
            println!("  📏 {:.1} avg line length | {} max line length", 
                lang_stats.avg_line_length,
                lang_stats.max_line_length.to_string().bright_white()
            );
        }
    }

    // Show top files if requested
    if let Some(ref metric) = args.top_files {
        show_top_files_enhanced(stats, metric);
    }

    // Show hotspots if requested
    if args.hotspots && !stats.hotspots.is_empty() {
        println!("\n{} Code Hotspots & Risk Analysis", "🔥".bright_red().bold());
        println!("{}", "─".repeat(90).bright_black());
        println!("  Files that may need attention based on complexity, size, and technical debt:\n");
        
        for (i, hotspot) in stats.hotspots.iter().enumerate() {
            let risk_indicator = match i {
                0..=2 => "🔴",  // High risk
                3..=6 => "🟡",  // Medium risk
                _ => "🟠",      // Lower risk
            };
            
            println!("  {} {}. {} | {} lines | {:.3} complexity | {} TODOs | {:.1} MI", 
                risk_indicator,
                (i + 1).to_string().bright_white(),
                hotspot.path.display().to_string().bright_red(),
                hotspot.lines.to_string().bright_white(),
                hotspot.complexity,
                hotspot.todos.to_string().bright_yellow(),
                hotspot.maintainability_index
            );
        }
    }

    println!("\n{}", "─".repeat(90).bright_black());
    println!("{} Analysis completed successfully! 🎉", "✅".bright_green().bold());
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

    println!("🚀 Initializing LOCO Ultra-Fast Analysis Engine...");
    println!("🎯 Target: {}", args.path.display().to_string().bright_white());

    let start_time = Instant::now();
    let files = collect_files_parallel(&args.path, &args);

    if files.is_empty() {
        println!("⚠️ No files found matching criteria.");
        return;
    }

    let thread_count = rayon::current_num_threads();
    println!("⚙️ Processing {} files with {} threads...", 
        files.len().to_string().bright_white(),
        thread_count.to_string().bright_white());

    // Progress bar setup
    let progress_bar = if args.progress {
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files ({eta})")
            .unwrap()
            .progress_chars("#>-"));
        Some(pb)
    } else {
        None
    };

    // Use DashMap for thread-safe concurrent access
    let languages = Arc::new(DashMap::<String, LanguageStats>::new());
    let files_info = Arc::new(DashMap::<usize, FileInfo>::new());
    let creation_dates = Arc::new(DashMap::<usize, u64>::new());
    let modification_dates = Arc::new(DashMap::<usize, u64>::new());
    
    let processed_count = Arc::new(AtomicUsize::new(0));
    let total_bytes_processed = Arc::new(AtomicU64::new(0));

    // Parallel processing with enhanced performance
    files.par_iter().enumerate().for_each(|(index, file_path)| {
        if let Some(extension) = file_path.extension().and_then(|e| e.to_str()) {
            if let Some(config) = LanguageConfig::get_config(extension) {
                if let Some((file_stats, file_info)) = analyze_file_advanced(file_path, &config, &args) {
                    let language = get_language_name(extension);

                    // Update language stats using DashMap
                    languages.entry(language).and_modify(|entry| {
                        entry.total_lines += file_stats.total_lines;
                        entry.code_lines += file_stats.code_lines;
                        entry.comment_lines += file_stats.comment_lines;
                        entry.blank_lines += file_stats.blank_lines;
                        entry.files += 1;
                        entry.total_size += file_stats.total_size;
                        
                        // Update weighted averages
                        let old_count = entry.files - 1;
                        if old_count > 0 {
                            entry.avg_line_length = (entry.avg_line_length * old_count as f64 + file_stats.avg_line_length) / entry.files as f64;
                            entry.complexity_score = (entry.complexity_score * old_count as f64 + file_stats.complexity_score) / entry.files as f64;
                            entry.maintainability_index = (entry.maintainability_index * old_count as f64 + file_stats.maintainability_index) / entry.files as f64;
                            entry.cyclomatic_complexity = (entry.cyclomatic_complexity * old_count as f64 + file_stats.cyclomatic_complexity) / entry.files as f64;
                        } else {
                            entry.avg_line_length = file_stats.avg_line_length;
                            entry.complexity_score = file_stats.complexity_score;
                            entry.maintainability_index = file_stats.maintainability_index;
                            entry.cyclomatic_complexity = file_stats.cyclomatic_complexity;
                        }
                        
                        entry.max_line_length = entry.max_line_length.max(file_stats.max_line_length);
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
                    }).or_insert(file_stats);

                    // Store file info
                    files_info.insert(index, file_info.clone());

                    // Store timestamps if available and requested
                    if args.time_analysis {
                        if let (Some(created), Some(modified)) = (file_info.created, file_info.modified) {
                            creation_dates.insert(index, created);
                            modification_dates.insert(index, modified);
                        }
                    }

                    // Update counters
                    total_bytes_processed.fetch_add(file_info.size, Ordering::Relaxed);
                }
            }
        }

        let _current = processed_count.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(ref pb) = progress_bar {
            pb.inc(1);
        }
    });

    if let Some(pb) = progress_bar {
        pb.finish_with_message("✅ Analysis completed!");
    }

    // Convert DashMap to HashMap for final stats
    let final_languages: HashMap<String, LanguageStats> = {
        let languages_ref = Arc::try_unwrap(languages).unwrap_or_else(|arc| (*arc).clone());
        languages_ref.into_iter().collect()
    };
    let final_files_info: Vec<FileInfo> = {
        let files_info_ref = Arc::try_unwrap(files_info).unwrap_or_else(|arc| (*arc).clone());
        files_info_ref.into_iter().map(|(_, v)| v).collect()
    };
    let final_creation_dates: Vec<u64> = {
        let creation_dates_ref = Arc::try_unwrap(creation_dates).unwrap_or_else(|arc| (*arc).clone());
        creation_dates_ref.into_iter().map(|(_, v)| v).collect()
    };
    let final_modification_dates: Vec<u64> = {
        let modification_dates_ref = Arc::try_unwrap(modification_dates).unwrap_or_else(|arc| (*arc).clone());
        modification_dates_ref.into_iter().map(|(_, v)| v).collect()
    };

    let analysis_time = start_time.elapsed().as_secs_f64();
    let total_bytes = total_bytes_processed.load(Ordering::Relaxed);

    // Calculate performance metrics
    let performance_metrics = PerformanceMetrics {
        files_per_second: files.len() as f64 / analysis_time,
        lines_per_second: final_languages.values().map(|s| s.total_lines).sum::<u64>() as f64 / analysis_time,
        bytes_per_second: total_bytes as f64 / analysis_time,
        peak_memory_usage: 0, // Would need system monitoring
        cpu_utilization: 0.0, // Would need system monitoring
    };

    // Get git stats if requested
    let git_info = if args.git_stats {
        get_git_stats(&args.path)
    } else {
        None
    };

    // Clone data for quality metrics calculation before moving
    let files_info_for_quality = final_files_info.clone();
    let languages_for_quality = final_languages.clone();

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
        languages: final_languages.clone(),
        analysis_time,
        git_info,
        creation_dates: final_creation_dates,
        modification_dates: final_modification_dates,
        files_info: final_files_info,
        hotspots,
        directory_stats: HashMap::new(), // Could be implemented for directory-level analysis
        performance_metrics,
        quality_metrics: calculate_quality_metrics(&ProjectStats {
            languages: languages_for_quality,
            total_files: files.len() as u64,
            total_lines: 0,
            total_size: total_bytes,
            analysis_time,
            git_info: None,
            creation_dates: vec![],
            modification_dates: vec![],
            files_info: files_info_for_quality,
            hotspots: vec![],
            directory_stats: HashMap::new(),
            performance_metrics: PerformanceMetrics {
                files_per_second: 0.0,
                lines_per_second: 0.0,
                bytes_per_second: 0.0,
                peak_memory_usage: 0,
                cpu_utilization: 0.0,
            },
            quality_metrics: QualityMetrics {
                overall_maintainability: 0.0,
                technical_debt_ratio: 0.0,
                test_coverage_estimate: 0.0,
                documentation_ratio: 0.0,
                code_duplication_ratio: 0.0,
            },
        }),
    };

    // Output results
    match args.format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&project_stats).unwrap();
            if let Some(output_path) = &args.output {
                fs::write(output_path, &json).unwrap();
                println!("📊 Results saved to: {}", output_path.display());
            } else {
                println!("{}", json);
            }
        },
        "html" => {
            let html = generate_html_report(&project_stats, &args);
            if let Some(output_path) = &args.output {
                fs::write(output_path, &html).unwrap();
                println!("📊 HTML report saved to: {}", output_path.display());
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
        let report_path = args.output.clone().unwrap_or_else(|| PathBuf::from("loco_advanced_report.html"));
        let html_report = generate_html_report(&project_stats, &args);
        fs::write(&report_path, &html_report).unwrap();
        println!("\n📊 Advanced HTML report saved to: {}", report_path.display().to_string().bright_green());
    }

    println!("\n{} LOCO Analysis completed successfully! 🎉", "✅".bright_green().bold());
    println!("📈 Processed {} files, {} lines in {:.3}s", 
        project_stats.total_files.to_string().bright_cyan(),
        project_stats.total_lines.to_string().bright_cyan(),
        analysis_time.to_string().bright_yellow()
    );
    }

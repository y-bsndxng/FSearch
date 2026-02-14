use anyhow::Result;
use clap::Parser;
use std::{env, path::PathBuf};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(author, version, about = "No-index file search (path substring)")]
struct Args {
    /// Query substring (matched against full path)
    query: String,

    /// Root directory to search (Windows default: system drive root, macOS/Linux default: /)
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Case-insensitive match
    #[arg(short, long)]
    ignore_case: bool,

    /// Include directories in results (default: files only)
    #[arg(long)]
    include_dirs: bool,

    /// Limit number of hits (0 = unlimited)
    #[arg(long, default_value_t = 0)]
    limit: usize,
}

fn default_root() -> PathBuf {
    if cfg!(windows) {
        // Windows: SystemDrive を優先（例: "C:"）
        if let Some(sd) = env::var_os("SystemDrive") {
            // "C:" -> "C:\"
            let mut s = sd.to_string_lossy().to_string();
            s.push('\\');
            PathBuf::from(s)
        } else {
            PathBuf::from(r"C:\")
        }
    } else {
        // macOS/Linux など
        PathBuf::from("/")
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let root = args.root.unwrap_or_else(default_root);

    let q = if args.ignore_case {
        args.query.to_lowercase()
    } else {
        args.query.clone()
    };

    let mut hits = 0usize;

    for entry in WalkDir::new(&root).follow_links(false).into_iter() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let ft = entry.file_type();
        if ft.is_dir() && !args.include_dirs {
            continue;
        }
        if !ft.is_file() && !ft.is_dir() {
            continue;
        }

        let path_str = entry.path().to_string_lossy();
        let hay = if args.ignore_case {
            path_str.to_lowercase()
        } else {
            path_str.to_string()
        };

        if hay.contains(&q) {
            println!("{}", path_str);
            hits += 1;
            if args.limit != 0 && hits >= args.limit {
                break;
            }
        }
    }

    Ok(())
}

use chrono::Local;
use std::{
    env,
    process::{Command, ExitCode, Stdio},
};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    // ---------------------------------------------------------
    // Help
    // ---------------------------------------------------------
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "/?"))
    {
        print_help();
        return ExitCode::SUCCESS;
    }

    // ---------------------------------------------------------
    // commit message
    //
    // 引数なし:
    //   2026-09-11_12-34_mod
    //
    // 引数あり:
    //   そのままcommit message
    // ---------------------------------------------------------
    let message = if args.is_empty() {
        format!("{}_mod", Local::now().format("%Y-%m-%d_%H-%M"))
    } else {
        args.join(" ")
    };

    // ---------------------------------------------------------
    // Git repository check
    // ---------------------------------------------------------
    if !is_git_repository() {
        eprintln!("Error: 現在のディレクトリは Git リポジトリではありません。");
        return ExitCode::FAILURE;
    }

    // ---------------------------------------------------------
    // 変更確認
    // ---------------------------------------------------------
    let changes = match get_git_changes() {
        Ok(changes) => changes,
        Err(err) => {
            eprintln!("git status Error: {err}");
            return ExitCode::FAILURE;
        }
    };

    if changes.trim().is_empty() {
        println!("変更はありません。");
        return ExitCode::SUCCESS;
    }

    println!("変更を検出しました:");
    println!();
    println!("{changes}");

    // ---------------------------------------------------------
    // git add .
    // ---------------------------------------------------------
    println!("--------------------------------------------------");
    println!("git add .");
    println!("--------------------------------------------------");

    if !run_git(&["add", "."]) {
        eprintln!("git add に失敗しました。");
        return ExitCode::FAILURE;
    }

    // ---------------------------------------------------------
    // git commit
    // ---------------------------------------------------------
    println!("--------------------------------------------------");
    println!("git commit -m \"{message}\"");
    println!("--------------------------------------------------");

    if !run_git(&["commit", "-m", &message]) {
        eprintln!("git commit に失敗しました。");
        return ExitCode::FAILURE;
    }

    // ---------------------------------------------------------
    // git push
    // ---------------------------------------------------------
    println!("--------------------------------------------------");
    println!("git push");
    println!("--------------------------------------------------");

    if !run_git(&["push"]) {
        eprintln!("git push に失敗しました。");
        return ExitCode::FAILURE;
    }

    println!();
    println!("Push 完了");
    println!("Commit: {message}");

    ExitCode::SUCCESS
}

// ============================================================
// Git repository check
// ============================================================

fn is_git_repository() -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

// ============================================================
// 変更取得
// ============================================================

fn get_git_changes() -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()?;

    if !output.status.success() {
        return Err("git status --porcelain に失敗しました".into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

// ============================================================
// Git command
// ============================================================

fn run_git(args: &[&str]) -> bool {
    match Command::new("git")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
    {
        Ok(status) => status.success(),

        Err(err) => {
            eprintln!("git 実行エラー: {err}");
            false
        }
    }
}

// ============================================================
// Help
// ============================================================

fn print_help() {
    println!(
        r#"mgit

Git の変更を確認し、

    git add .
    git commit -m "message"
    git push

を順番に実行します。

USAGE:

    mgit
    mgit <commit message>

EXAMPLES:

    mgit

        → 現在日時から自動生成
          2026-09-11_12-34_mod

    mgit fix bug

        → "fix bug"

    mgit "translation toggle"

        → "translation toggle"

OPTIONS:

    -h, --help, /?
        このヘルプを表示

変更がない場合は何もせず終了します。
"#
    );
}

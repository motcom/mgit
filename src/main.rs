use chrono::Local;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();

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
    // --all / -a
    // ---------------------------------------------------------
    let all = args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-a" | "--all"));

    args.retain(|arg| !matches!(arg.as_str(), "-a" | "--all"));

    // ---------------------------------------------------------
    // Commit message
    //
    // 引数なし:
    //   2026-09-11_12-34_mod
    //
    // 引数あり:
    //   mgit fix bug
    //   -> "fix bug"
    // ---------------------------------------------------------
    let message = if args.is_empty() {
        format!("{}_mod", Local::now().format("%Y-%m-%d_%H-%M"))
    } else {
        args.join(" ")
    };

    // ---------------------------------------------------------
    // --all
    //
    // 現在のディレクトリ直下だけを見る
    // ---------------------------------------------------------
    if all {
        return run_all(&message);
    }

    // ---------------------------------------------------------
    // 通常モード
    //
    // 現在いるGitリポジトリを処理
    // ---------------------------------------------------------
    let current = match env::current_dir() {
        Ok(path) => path,

        Err(err) => {
            eprintln!("現在のディレクトリを取得できません: {err}");
            return ExitCode::FAILURE;
        }
    };

    if !is_git_repository(&current) {
        eprintln!("現在のディレクトリは Git リポジトリではありません。");
        return ExitCode::FAILURE;
    }

    if process_repository(&current, &message) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// ============================================================
// --all
//
// 現在ディレクトリ直下のGitリポジトリだけ処理
//
// 例:
//
// Programs/
// ├─ ask/.git          ← 対象
// ├─ mgit/.git         ← 対象
// ├─ runlan/.git       ← 対象
// └─ rust/
//     └─ test/.git     ← 対象外
//
// ============================================================

fn run_all(message: &str) -> ExitCode {
    let current = match env::current_dir() {
        Ok(path) => path,

        Err(err) => {
            eprintln!("現在のディレクトリを取得できません: {err}");
            return ExitCode::FAILURE;
        }
    };

    let repos = find_git_repositories(&current);

    if repos.is_empty() {
        println!("直下に Git リポジトリが見つかりませんでした。");
        return ExitCode::SUCCESS;
    }

    println!();
    println!("{} 個の Git リポジトリを検出しました。", repos.len());

    for repo in &repos {
        println!("  {}", repo.display());
    }

    println!();

    let mut success_count = 0;
    let mut failed_count = 0;
    let mut unchanged_count = 0;

    // ---------------------------------------------------------
    // 各repository処理
    // ---------------------------------------------------------
    for repo in repos {
        println!();
        println!("============================================================");
        println!("{}", repo.display());
        println!("============================================================");

        match process_repository_result(&repo, message) {
            RepositoryResult::Success => {
                success_count += 1;
            }

            RepositoryResult::Unchanged => {
                unchanged_count += 1;
            }

            RepositoryResult::Failed => {
                failed_count += 1;
            }
        }
    }

    // ---------------------------------------------------------
    // 結果
    // ---------------------------------------------------------
    println!();
    println!("============================================================");
    println!("結果");
    println!("============================================================");
    println!("変更なし : {unchanged_count}");
    println!("完了     : {success_count}");
    println!("失敗     : {failed_count}");

    if failed_count == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// ============================================================
// 現在ディレクトリ直下のGit repository検索
//
// 再帰検索しない。
// ============================================================

fn find_git_repositories(root: &Path) -> Vec<PathBuf> {
    let mut repos = Vec::new();

    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,

        Err(err) => {
            eprintln!("ディレクトリを読み込めません: {err}");
            return repos;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // ディレクトリ以外は無視
        if !path.is_dir() {
            continue;
        }

        // -----------------------------------------------------
        // 直下に .git があるフォルダだけ対象
        //
        // ここでは再帰しない。
        // -----------------------------------------------------
        if path.join(".git").exists() {
            repos.push(path);
        }
    }

    // 名前順に並べる
    repos.sort();

    repos
}

// ============================================================
// Repository Result
// ============================================================

enum RepositoryResult {
    Success,
    Unchanged,
    Failed,
}

// ============================================================
// 1 repository処理
// ============================================================

fn process_repository(repo: &Path, message: &str) -> bool {
    !matches!(
        process_repository_result(repo, message),
        RepositoryResult::Failed
    )
}

// ============================================================
// 1 repository処理
//
// git status
// ↓
// git add .
// ↓
// git commit
// ↓
// remoteあり
//     ↓
// git push
//
// remoteなし
//     ↓
// commitまでで終了
//
// ============================================================

fn process_repository_result(repo: &Path, message: &str) -> RepositoryResult {
    // ---------------------------------------------------------
    // Git repository確認
    // ---------------------------------------------------------
    if !is_git_repository(repo) {
        eprintln!("Git リポジトリではありません。");
        return RepositoryResult::Failed;
    }

    // ---------------------------------------------------------
    // 変更確認
    //
    // git diff では untracked file を検出できないので、
    // git status --porcelain を使う。
    // ---------------------------------------------------------
    let changes = match get_git_changes(repo) {
        Ok(changes) => changes,

        Err(err) => {
            eprintln!("git status Error: {err}");
            return RepositoryResult::Failed;
        }
    };

    // ---------------------------------------------------------
    // 変更なし
    // ---------------------------------------------------------
    if changes.trim().is_empty() {
        println!("変更なし");
        return RepositoryResult::Unchanged;
    }

    println!();
    println!("変更:");
    println!();
    println!("{changes}");

    // ---------------------------------------------------------
    // git add .
    // ---------------------------------------------------------
    println!("------------------------------------------------------------");
    println!("git add .");
    println!("------------------------------------------------------------");

    if !run_git(repo, &["add", "."]) {
        eprintln!("git add に失敗しました。");
        return RepositoryResult::Failed;
    }

    // ---------------------------------------------------------
    // git commit
    // ---------------------------------------------------------
    println!();
    println!("------------------------------------------------------------");
    println!("git commit -m \"{message}\"");
    println!("------------------------------------------------------------");

    if !run_git(repo, &["commit", "-m", message]) {
        eprintln!("git commit に失敗しました。");
        return RepositoryResult::Failed;
    }

    // ---------------------------------------------------------
    // Remote確認
    // ---------------------------------------------------------
    if !has_remote(repo) {
        println!();
        println!("Remote が無いため push をスキップしました。");
        println!("Commit 完了: {message}");

        return RepositoryResult::Success;
    }

    // ---------------------------------------------------------
    // git push
    // ---------------------------------------------------------
    println!();
    println!("------------------------------------------------------------");
    println!("git push");
    println!("------------------------------------------------------------");

    if !run_git(repo, &["push"]) {
        eprintln!("git push に失敗しました。");
        return RepositoryResult::Failed;
    }

    println!();
    println!("Push 完了");
    println!("Commit: {message}");

    RepositoryResult::Success
}

// ============================================================
// Git repository判定
// ============================================================

fn is_git_repository(dir: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

// ============================================================
// Git変更取得
//
// git status --porcelain:
//
// M  src/main.rs
// ?? new_file.txt
//
// のような結果を返す。
// ============================================================

fn get_git_changes(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Err("git status --porcelain に失敗しました".into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

// ============================================================
// Remote確認
//
// git remote
//
// origin
//
// のように何か返ればtrue。
// ============================================================

fn has_remote(dir: &Path) -> bool {
    let output = Command::new("git").arg("remote").current_dir(dir).output();

    match output {
        Ok(output) => {
            if !output.status.success() {
                return false;
            }

            !String::from_utf8_lossy(&output.stdout).trim().is_empty()
        }

        Err(_) => false,
    }
}

// ============================================================
// Git command実行
// ============================================================

fn run_git(dir: &Path, args: &[&str]) -> bool {
    match Command::new("git")
        .args(args)
        .current_dir(dir)
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

Git の変更を確認して、

    git add .
    git commit -m "message"
    git push

を自動実行します。


USAGE:

    mgit

    mgit <commit message>

    mgit -a
    mgit --all

    mgit -a <commit message>


------------------------------------------------------------
通常実行
------------------------------------------------------------

    mgit

現在いるGitリポジトリを処理します。

引数なしの場合は現在日時からcommit messageを作ります。

例:

    2026-09-11_12-34_mod


------------------------------------------------------------
Commit message指定
------------------------------------------------------------

    mgit fix bug

Commit:

    fix bug


または:

    mgit "fix translation toggle"


------------------------------------------------------------
--all
------------------------------------------------------------

    mgit --all

現在のディレクトリ直下だけを検索して、
Gitリポジトリをすべて処理します。


例:

    Programs/
    ├─ ask/.git
    ├─ catext/.git
    ├─ mgit/.git
    ├─ runlan/.git
    └─ rust/
        └─ test/.git


この場合:

    ask
    catext
    mgit
    runlan

だけを処理します。

rust/test は2階層下なので対象外です。


------------------------------------------------------------
--all + message
------------------------------------------------------------

    mgit --all update

すべての対象repositoryを

    update

というmessageでcommitします。


------------------------------------------------------------
Remoteが無い場合
------------------------------------------------------------

    git add .
    git commit

までは実行します。

git push はスキップします。


------------------------------------------------------------
変更がない場合
------------------------------------------------------------

何もcommitせず終了します。

--allの場合はそのrepositoryをスキップして、
次のrepositoryへ進みます。


OPTIONS:

    -a, --all

        現在ディレクトリ直下の
        Git repositoryをすべて処理

    -h, --help, /?

        このヘルプを表示
"#
    );
}

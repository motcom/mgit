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
    // --force / -f
    // ---------------------------------------------------------
    let force = args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-f" | "--force"));

    args.retain(|arg| !matches!(arg.as_str(), "-f" | "--force"));

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
    // ---------------------------------------------------------
    if all {
        return run_all(&message, force);
    }

    // ---------------------------------------------------------
    // 通常モード
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

    if process_repository(&current, &message, force) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// ============================================================
// --all
//
// 現在ディレクトリ直下のGitリポジトリだけ処理
// ============================================================

fn run_all(message: &str, force: bool) -> ExitCode {
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

    for repo in repos {
        println!();
        println!("============================================================");
        println!("{}", repo.display());
        println!("============================================================");

        match process_repository_result(&repo, message, force) {
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

        if !path.is_dir() {
            continue;
        }

        if path.join(".git").exists() {
            repos.push(path);
        }
    }

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
// Remote State
// ============================================================

#[derive(Debug)]
enum RemoteState {
    /// upstream が設定されていない
    NoUpstream,

    /// local と remote が同一
    UpToDate,

    /// local の方が進んでいる
    Ahead(u64),

    /// remote の方が進んでいる
    Behind(u64),

    /// local と remote の両方に別々のcommitがある
    Diverged { ahead: u64, behind: u64 },
}

// ============================================================
// 1 repository処理
// ============================================================

fn process_repository(repo: &Path, message: &str, force: bool) -> bool {
    !matches!(
        process_repository_result(repo, message, force),
        RepositoryResult::Failed
    )
}

// ============================================================
// 1 repository処理
//
// remoteあり:
//
//     git fetch
//         ↓
//     remoteとの差を確認
//         ↓
//     remoteが進んでいる
//         ↓
//     通常:
//         エラーで停止
//
//     --force:
//         続行
//
//         ↓
//
//     git status
//         ↓
//     変更あり:
//         git add .
//         git commit
//
//         ↓
//
//     push が必要:
//         git push
//
//     --force:
//         git push --force
//
// ============================================================

fn process_repository_result(repo: &Path, message: &str, force: bool) -> RepositoryResult {
    // ---------------------------------------------------------
    // Git repository確認
    // ---------------------------------------------------------
    if !is_git_repository(repo) {
        eprintln!("Git リポジトリではありません。");
        return RepositoryResult::Failed;
    }

    let remote_exists = has_remote(repo);

    let mut remote_state = RemoteState::NoUpstream;

    // ---------------------------------------------------------
    // Remoteがある場合
    // 先にfetchしてremoteとの差を確認する
    // ---------------------------------------------------------
    if remote_exists {
        println!();
        println!("------------------------------------------------------------");
        println!("git fetch");
        println!("------------------------------------------------------------");

        if !run_git(repo, &["fetch"]) {
            eprintln!();
            eprintln!("git fetch に失敗しました。");
            eprintln!("Remote の状態を確認できないため処理を中止します。");
            return RepositoryResult::Failed;
        }

        remote_state = match get_remote_state(repo) {
            Ok(state) => state,

            Err(err) => {
                eprintln!("Remote 状態確認に失敗しました: {err}");
                return RepositoryResult::Failed;
            }
        };

        println!();
        println!("Remote 状態:");

        match &remote_state {
            RemoteState::NoUpstream => {
                println!("  upstream 未設定");
            }

            RemoteState::UpToDate => {
                println!("  local と remote は同じです");
            }

            RemoteState::Ahead(count) => {
                println!("  local が {count} commit 進んでいます");
            }

            RemoteState::Behind(count) => {
                println!("  remote が {count} commit 進んでいます");
            }

            RemoteState::Diverged { ahead, behind } => {
                println!("  local  : {ahead} commit 進んでいます");
                println!("  remote : {behind} commit 進んでいます");
            }
        }

        // -----------------------------------------------------
        // Remoteの方が進んでいる場合
        // -----------------------------------------------------
        match &remote_state {
            RemoteState::Behind(count) => {
                if !force {
                    eprintln!();
                    eprintln!("============================================================");
                    eprintln!("Push できません");
                    eprintln!("============================================================");
                    eprintln!();
                    eprintln!("Remote が {count} commit 進んでいます。");
                    eprintln!();
                    eprintln!("このまま push すると rejected されるため、");
                    eprintln!("commit 前に処理を停止しました。");
                    eprintln!();
                    eprintln!("Remote の変更を取り込む場合:");
                    eprintln!();
                    eprintln!("    git pull --rebase");
                    eprintln!();
                    eprintln!("Remote を無視してローカルで強制上書きする場合:");
                    eprintln!();
                    eprintln!("    mgit --force");
                    eprintln!();
                    eprintln!("または:");
                    eprintln!();
                    eprintln!("    mgit -f");
                    eprintln!();

                    return RepositoryResult::Failed;
                }

                println!();
                println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                println!("WARNING: --force");
                println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                println!();
                println!("Remote が {count} commit 進んでいます。");
                println!("Remote の変更をローカルで強制上書きします。");
            }

            RemoteState::Diverged { ahead, behind } => {
                if !force {
                    eprintln!();
                    eprintln!("============================================================");
                    eprintln!("Push できません");
                    eprintln!("============================================================");
                    eprintln!();
                    eprintln!("Local と Remote が分岐しています。");
                    eprintln!();
                    eprintln!("local  : {ahead} commit");
                    eprintln!("remote : {behind} commit");
                    eprintln!();
                    eprintln!("このまま push すると rejected されるため、");
                    eprintln!("commit 前に処理を停止しました。");
                    eprintln!();
                    eprintln!("Remote の変更を取り込む場合:");
                    eprintln!();
                    eprintln!("    git pull --rebase");
                    eprintln!();
                    eprintln!("Remote を無視してローカルで強制上書きする場合:");
                    eprintln!();
                    eprintln!("    mgit --force");
                    eprintln!();
                    eprintln!("または:");
                    eprintln!();
                    eprintln!("    mgit -f");
                    eprintln!();

                    return RepositoryResult::Failed;
                }

                println!();
                println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                println!("WARNING: --force");
                println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                println!();
                println!("Local と Remote が分岐しています。");
                println!("Remote の変更をローカルで強制上書きします。");
            }

            _ => {}
        }
    }

    // ---------------------------------------------------------
    // 作業ツリー変更確認
    // ---------------------------------------------------------
    let changes = match get_git_changes(repo) {
        Ok(changes) => changes,

        Err(err) => {
            eprintln!("git status Error: {err}");
            return RepositoryResult::Failed;
        }
    };

    let has_changes = !changes.trim().is_empty();

    // ---------------------------------------------------------
    // 変更あり
    // ---------------------------------------------------------
    if has_changes {
        println!();
        println!("変更:");
        println!();
        println!("{changes}");

        // -----------------------------------------------------
        // git add .
        // -----------------------------------------------------
        println!("------------------------------------------------------------");
        println!("git add .");
        println!("------------------------------------------------------------");

        if !run_git(repo, &["add", "."]) {
            eprintln!("git add に失敗しました。");
            return RepositoryResult::Failed;
        }

        // -----------------------------------------------------
        // git commit
        // -----------------------------------------------------
        println!();
        println!("------------------------------------------------------------");
        println!("git commit -m \"{message}\"");
        println!("------------------------------------------------------------");

        if !run_git(repo, &["commit", "-m", message]) {
            eprintln!("git commit に失敗しました。");
            return RepositoryResult::Failed;
        }
    }

    // ---------------------------------------------------------
    // Remoteなし
    // ---------------------------------------------------------
    if !remote_exists {
        if has_changes {
            println!();
            println!("Remote が無いため push をスキップしました。");
            println!("Commit 完了: {message}");

            return RepositoryResult::Success;
        }

        println!("変更なし");
        return RepositoryResult::Unchanged;
    }

    // ---------------------------------------------------------
    // Pushが必要か
    //
    // 作業ツリーに変更があった
    //      → 新しいcommitができたのでpush
    //
    // 変更なしでもlocalがahead
    //      → 前回push失敗などなのでpush
    //
    // force + remoteがahead/diverged
    //      → 強制push
    // ---------------------------------------------------------
    let should_push = if has_changes {
        true
    } else {
        match remote_state {
            RemoteState::Ahead(_) => true,

            RemoteState::Behind(_) if force => true,

            RemoteState::Diverged { .. } if force => true,

            RemoteState::NoUpstream => true,

            _ => false,
        }
    };

    // ---------------------------------------------------------
    // push不要
    // ---------------------------------------------------------
    if !should_push {
        println!();
        println!("変更なし");
        println!("Local と Remote は同期済みです。");

        return RepositoryResult::Unchanged;
    }

    // ---------------------------------------------------------
    // git push
    // ---------------------------------------------------------
    println!();
    println!("------------------------------------------------------------");

    if force {
        println!("git push --force");
    } else {
        println!("git push");
    }

    println!("------------------------------------------------------------");

    let push_success = if force {
        run_git(repo, &["push", "--force"])
    } else {
        run_git(repo, &["push"])
    };

    if !push_success {
        eprintln!();
        eprintln!("git push に失敗しました。");
        return RepositoryResult::Failed;
    }

    println!();
    println!("Push 完了");

    if has_changes {
        println!("Commit: {message}");
    }

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
// Remoteとの状態確認
//
// git rev-list --left-right --count HEAD...@{u}
//
// 例:
//
//     2    0
//
// local が2commit進んでいる
//
//     0    3
//
// remote が3commit進んでいる
//
//     2    3
//
// local / remote が分岐
// ============================================================

fn get_remote_state(dir: &Path) -> Result<RemoteState, Box<dyn std::error::Error>> {
    // ---------------------------------------------------------
    // upstream確認
    // ---------------------------------------------------------
    let upstream = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if !upstream.success() {
        return Ok(RemoteState::NoUpstream);
    }

    // ---------------------------------------------------------
    // local / remote commit差分取得
    // ---------------------------------------------------------
    let output = Command::new("git")
        .args(["rev-list", "--left-right", "--count", "HEAD...@{u}"])
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Err("git rev-list --left-right --count に失敗しました".into());
    }

    let text = String::from_utf8_lossy(&output.stdout);

    let mut parts = text.split_whitespace();

    let ahead: u64 = parts.next().ok_or("ahead の取得に失敗しました")?.parse()?;

    let behind: u64 = parts.next().ok_or("behind の取得に失敗しました")?.parse()?;

    let state = match (ahead, behind) {
        (0, 0) => RemoteState::UpToDate,

        (ahead, 0) => RemoteState::Ahead(ahead),

        (0, behind) => RemoteState::Behind(behind),

        (ahead, behind) => RemoteState::Diverged { ahead, behind },
    };

    Ok(state)
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

Remote がある場合は最初に

    git fetch

を行い、Local と Remote の状態を確認します。


USAGE:

    mgit

    mgit <commit message>

    mgit -a
    mgit --all

    mgit -f
    mgit --force

    mgit -f <commit message>

    mgit -a -f
    mgit --all --force


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
Remoteチェック
------------------------------------------------------------

Remote が存在する場合、最初に

    git fetch

を実行します。

その後、

    HEAD
    upstream

のcommit差分を確認します。


Remote の方が進んでいる場合は、

    git add
    git commit

を実行する前にエラーで停止します。

例:

    Remote が 2 commit 進んでいます。

    Push できません。


Remoteを取り込む場合:

    git pull --rebase


------------------------------------------------------------
--force / -f
------------------------------------------------------------

    mgit --force

または:

    mgit -f

Remote が進んでいたり分岐していても、

    git push --force

を実行します。


例:

    mgit -f

    mgit -f fix

    mgit --force "local version"


注意:

    --force は Remote 側だけに存在するcommitを
    消す可能性があります。


------------------------------------------------------------
Pushだけ残っている場合
------------------------------------------------------------

例えば前回、

    git commit

までは成功したが、

    git push

だけ失敗した場合。

作業ファイルに変更がなくても Local が Remote より
進んでいれば、

    mgit

だけで push を再実行します。

つまり、

    変更なし

とは判定しません。


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
--all + --force
------------------------------------------------------------

    mgit --all --force

または:

    mgit -a -f

各repositoryに対してRemoteとの差を確認し、
必要なら

    git push --force

します。


------------------------------------------------------------
Remoteが無い場合
------------------------------------------------------------

変更があれば、

    git add .
    git commit

までは実行します。

git push はスキップします。


------------------------------------------------------------
変更がない場合
------------------------------------------------------------

作業ファイルに変更がなくても、

Local が Remote より進んでいる場合:

    git push

を実行します。


Local と Remote が同じ場合:

    変更なし

で終了します。


------------------------------------------------------------
OPTIONS
------------------------------------------------------------

    -a, --all

        現在ディレクトリ直下の
        Git repositoryをすべて処理


    -f, --force

        git push --force を使用

        Remote側のcommitが消える可能性があるため注意


    -h, --help, /?

        このヘルプを表示
"#
    );
}

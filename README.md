# mgit
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

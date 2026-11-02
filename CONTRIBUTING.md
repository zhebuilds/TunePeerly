# Contributing

## Agree on an issue first

Search the [open issues](https://github.com/zhebuilds/TunePeerly/issues). If the change is not already there, open an issue and describe the bug or the change you want.

Wait until the discussion reaches a conclusion and the issue is confirmed. Start development after that. Do not open a pull request for a change that has not been confirmed.

Report a security problem in private, as described in [SECURITY.md](SECURITY.md). Do not open a public issue for it.

## Fork, and keep dev the same as this project

Fork [zhebuilds/TunePeerly](https://github.com/zhebuilds/TunePeerly), then clone your fork and point `upstream` at this project:

```sh
git clone https://github.com/<your-account>/TunePeerly.git
cd TunePeerly
git remote add upstream https://github.com/zhebuilds/TunePeerly.git
git fetch upstream
git checkout dev
git merge upstream/dev
git push origin dev
```

`origin` is your fork. `upstream` is this project. Your `dev` stays a copy of this project's `dev`. Do not commit on `dev`.

When this project's `dev` moves, update yours before you branch:

```sh
git checkout dev
git fetch upstream
git merge upstream/dev
git push origin dev
```

## Branch

Create the branch from that updated `dev`. Use one branch for one confirmed issue.

```sh
git checkout -b fix/short-name
```

## Develop

The commands are the same as in the README. Run them from the repository root:

```sh
npm install
npm run tauri dev
npm test
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Keep the existing source notice, `Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.`

## Update the branch, then open a pull request

Commit on the feature branch and push it to your fork:

```sh
git push -u origin fix/short-name
```

If this project's `dev` has moved, bring those commits into your branch and push the branch again:

```sh
git fetch upstream
git rebase upstream/dev
git push --force-with-lease origin fix/short-name
```

`--force-with-lease` is only for that branch on your fork.

Open the pull request against `zhebuilds/TunePeerly`, with `dev` as the base. Link the confirmed issue, say what changed, and say how you checked it.

After the pull request is merged, update your `dev` from `upstream` again.

Contributions are under the [GNU AGPLv3](https://www.gnu.org/licenses/agpl-3.0.html) and [ADDITIONAL_TERMS.md](ADDITIONAL_TERMS.md).

# 贡献

## 先确定 issue

先看[已有的 issue](https://github.com/zhebuilds/TunePeerly/issues)。还没有的话，开一个 issue，写清缺陷或你想做的改动。

等讨论有了结论，并且这个 issue 被确认之后，再开始开发。未经确认的改动不要直接开 pull request。

安全问题按 [SECURITY.md](SECURITY.md) 私下报告，不要开公开的 issue。

## Fork，并让主分支与本项目一致

Fork [zhebuilds/TunePeerly](https://github.com/zhebuilds/TunePeerly)，克隆你自己的仓库，并把 `upstream` 指向本项目：

```sh
git clone https://github.com/<你的账号>/TunePeerly.git
cd TunePeerly
git remote add upstream https://github.com/zhebuilds/TunePeerly.git
git fetch upstream
git checkout dev
git merge upstream/dev
git push origin dev
```

`origin` 是你的 fork。`upstream` 是本项目。你的 `dev` 保持为本项目 `dev` 的一份副本。不要在 `dev` 上提交。

本项目的 `dev` 有新提交时，开分支之前先更新你的 `dev`：

```sh
git checkout dev
git fetch upstream
git merge upstream/dev
git push origin dev
```

## 分支

从更新后的 `dev` 建分支。一个已确认的 issue 对应一个分支。

```sh
git checkout -b fix/short-name
```

## 开发

命令和 README 里的相同，在仓库根目录执行：

```sh
npm install
npm run tauri dev
npm test
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

保留源文件里已有的声明：`Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.`

## 更新分支，然后开 pull request

在功能分支上提交，并推到你的 fork：

```sh
git push -u origin fix/short-name
```

如果本项目的 `dev` 又有新提交，先合进你的分支，再推一次这个分支：

```sh
git fetch upstream
git rebase upstream/dev
git push --force-with-lease origin fix/short-name
```

`--force-with-lease` 只用于你 fork 上的这个分支。

向 `zhebuilds/TunePeerly` 开 pull request，目标分支是 `dev`。写上已确认的 issue、改了什么，以及你如何检查的。

Pull request 合并之后，再次用 `upstream` 更新你的 `dev`。

贡献适用 [GNU AGPLv3](https://www.gnu.org/licenses/agpl-3.0.html) 和 [ADDITIONAL_TERMS.md](ADDITIONAL_TERMS.md)。

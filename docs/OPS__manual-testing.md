# OPS__manual-testing

面向一次完整人工验收的手工流程。默认在 Linux 本地执行；Windows 与 macOS 的差异单独列出。所有操作都在临时目录中完成，不触碰真实的系统配置文件。

## 0. 准备

```sh
export REPO=/path/to/bakon
export BAKON="$REPO/target/debug/bakon"       # 崩溃注入需要 debug 构建
export RELEASE="$REPO/target/release/bakon"   # 发布验收使用
export T=$(mktemp -d /tmp/bakon-manual.XXXXXX)
cd "$T"
```

```sh
cargo build --workspace
cargo build --release -p bakon
"$BAKON" version
"$RELEASE" version
```

预期：两条 `version` 都输出版本与 `x86_64-unknown-linux-gnu` 一类的目标三元组。

## 1. 入口与帮助

```sh
"$BAKON" --help
"$BAKON" help edit
```

预期：列出 16 个命令：`init`、`version`、`edit`、`revert`、`log`、`ls`、`path`、`store`、`show`、`dump`、`diff`、`prune`、`mv`、`hook`、`config`、`verify`。

## 2. 初始化与归属

```sh
"$BAKON" --store ./store init
"$BAKON" --store ./store init
mkdir foreign && printf x > foreign/notes.txt
"$BAKON" --store ./foreign init; echo "exit=$?"
```

预期：

- 第一次输出 `initialized`，第二次输出 `already initialized`，均退出 0。
- 对非空且无标记的目录退出 6，并提示不是 bakon 存储。
- `store/` 下存在 `BAKON_STORE`、`files/`、`objects/`、`trash/`。

## 3. 纳管与历史

```sh
printf 'one\n' > note.txt
printf '#!/bin/sh\nprintf "two\\n" > "$1"\n' > editor.sh
chmod +x editor.sh

EDITOR="$T/editor.sh" "$BAKON" --store ./store edit ./note.txt
"$BAKON" --store ./store log ./note.txt
```

预期：首次 `edit` 输出 `tracked ... baseline version 0`，随后保存版本 1；`log` 显示版本 1、0，来源分别为 `edit`、`baseline`。

```sh
printf '#!/bin/sh\nexit 0\n' > noop.sh
chmod +x noop.sh
EDITOR="$T/noop.sh" "$BAKON" --store ./store edit ./note.txt
```

预期：输出 `no changes`，版本数不变。

## 4. 查看与比较

```sh
"$BAKON" --store ./store show ./note.txt 0
"$BAKON" --store ./store dump ./note.txt 0 > exported.txt
"$BAKON" --store ./store diff ./note.txt
echo "diff_exit=$?"
"$BAKON" --store ./store diff ./note.txt 0 1
echo "diff_exit=$?"
"$BAKON" --store ./store diff ./note.txt 0 99
echo "missing_exit=$?"
```

预期：

- `show` 输出原始字节；`dump` 结果与版本 0 内容一致。
- 版本 0 与 1 有差异时退出 1。
- 不存在的版本退出 3，而不是 1。

二进制差异：

```sh
printf '\000\001\002' > blob.bin
printf '#!/bin/sh\nprintf "\\000\\003" > "$1"\n' > bin-editor.sh
chmod +x bin-editor.sh
EDITOR="$T/bin-editor.sh" "$BAKON" --store ./store edit ./blob.bin
"$BAKON" --store ./store diff ./blob.bin 0 1
echo "binary_diff_exit=$?"
```

预期：stderr 提示二进制文件不同，退出 1。

## 5. 回退与分歧

```sh
"$BAKON" --store ./store revert ./note.txt 0
cat note.txt
```

预期：文件内容回到版本 0，`log` 新增一条 `revert` 版本；历史不删除。

```sh
printf 'outside\n' > note.txt
"$BAKON" --store ./store revert ./note.txt 0; echo "refuse_exit=$?"
"$BAKON" --store ./store revert ./note.txt 0 --force; echo "force_exit=$?"
"$BAKON" --store ./store --format plain log ./note.txt
```

预期：

- 未加 `--force` 时退出 4，文件保持 `outside`。
- 加 `--force` 时退出 0；日志出现 `salvage`，随后是新的 `revert`。
- `show` 能读回 `outside` 的 salvage 版本。

缺失目标文件：

```sh
rm note.txt
"$BAKON" --store ./store revert ./note.txt 0
cat note.txt
```

预期：文件被重新创建，内容为版本 0。

## 6. 符号链接与路径

```sh
mkdir -p real linkdir
printf 'target\n' > real/file.txt
ln -s ../real linkdir/link
printf '#!/bin/sh\nprintf "changed\\n" > "$1"\n' > link-editor.sh
chmod +x link-editor.sh

EDITOR="$T/link-editor.sh" "$BAKON" --store ./store edit ./linkdir/link
test -L linkdir/link && echo "link_kept"
cat real/file.txt
```

预期：链接本身仍是符号链接，真实目标内容变为 `changed`。

```sh
"$BAKON" --store ./store edit ./linkdir/link/../file.txt; echo "dotdot_exit=$?"
```

预期：退出 2；提示 `..` 前经过符号链接；两个文件都未被修改。

## 7. 裁剪与对象回收

```sh
printf '[retention]\nmax_versions = 2\n' > limited.toml
printf 'v0\n' > trim.txt
for n in 1 2 3 4 5; do
  printf '#!/bin/sh\nprintf "v%s\\n" > "$1"\n' "$n" > trim-editor.sh
  chmod +x trim-editor.sh
  EDITOR="$T/trim-editor.sh" "$BAKON" --store ./store --config limited.toml edit ./trim.txt
done

"$BAKON" --store ./store --config limited.toml --format plain log ./trim.txt
"$BAKON" --store ./store --config limited.toml verify
"$BAKON" --store ./store --config limited.toml prune --dry-run
"$BAKON" --store ./store --config limited.toml store size --format json
```

预期：

- `log` 只剩最新 2 个版本。
- `verify` 返回 0。
- `prune --dry-run` 不改变版本。
- `store size` 中的文件数与对象数应与当前版本数一致，不随历史次数无限增长。

> 注意：`trim-editor.sh` 每次写的是 `v$n`，如果内容重复，不会产生新版本；确认 `log` 的版本号连续递增。

## 8. 改名与身份

```sh
"$BAKON" --store ./store mv ./trim.txt ./trim-renamed.txt
"$BAKON" --store ./store log ./trim-renamed.txt
"$BAKON" --store ./store log ./trim.txt; echo "old_exit=$?"
printf 'new\n' > trim.txt
EDITOR="$T/noop.sh" "$BAKON" --store ./store edit ./trim.txt
"$BAKON" --store ./store --format json path ./trim.txt
"$BAKON" --store ./store --format json path ./trim-renamed.txt
```

预期：

- `mv` 只改记录路径，磁盘文件不移动。
- 旧路径不再有历史，退出 3。
- 重新纳管旧路径产生新的 id；两个 `path` 输出的 id 不同。

## 9. 命令与格式

```sh
"$BAKON" --store ./store ls
"$BAKON" --store ./store path ./trim-renamed.txt
"$BAKON" --store ./store store path
"$BAKON" --store ./store store size
"$BAKON" --store ./store --format json ls
"$BAKON" --store ./store --format json prune
```

预期：

- `ls` 按路径排序。
- `path` 输出 path、id、manifest。
- `store path/size` 输出存储位置或字节/文件数。
- `--format json` 只输出 JSON；空 `prune` 输出 `{"dry_run":false,"files":[]}` 一类对象。

## 10. 钩子

```sh
"$BAKON" --store ./store hook set ./trim-renamed.txt 'printf "%s %s %s\\n" "$BAKON_FILE" "$BAKON_VERSION" "$BAKON_SOURCE" >> hook.log'
printf '#!/bin/sh\nprintf "hook-change\\n" > "$1"\n' > hook-editor.sh
chmod +x hook-editor.sh
EDITOR="$T/hook-editor.sh" "$BAKON" --store ./store edit ./trim-renamed.txt
cat hook.log
"$BAKON" --store ./store hook show ./trim-renamed.txt
```

预期：钩子文件包含目标绝对路径、新版本号与 `edit`；钩子执行成功时命令退出 0。

```sh
"$BAKON" --store ./store hook set ./trim-renamed.txt 'exit 9'
EDITOR="$T/hook-editor.sh" "$BAKON" --store ./store edit ./trim-renamed.txt
echo "hook_fail_exit=$?"
"$BAKON" --store ./store hook unset ./trim-renamed.txt
```

预期：退出 5；版本仍然保存；`--no-hook` 可跳过。

递归：

```sh
"$BAKON" --store ./store hook set ./trim-renamed.txt "$BAKON --store ./store log ./trim-renamed.txt"
EDITOR="$T/hook-editor.sh" "$BAKON" --store ./store edit ./trim-renamed.txt
echo "recursive_exit=$?"
```

预期：钩子不递归；子进程看到 `BAKON_HOOK=1` 后跳过钩子；命令正常结束。

## 11. 配置

```sh
"$BAKON" --config ./config.toml config init
cat config.toml
"$BAKON" --config ./config.toml config show
"$BAKON" --config ./config.toml --format json config show
printf 'broken = [\n' > bad.toml
"$BAKON" --config ./bad.toml config show; echo "bad_config_exit=$?"
```

预期：`config init` 生成模板；`config show` 显示生效的 config、store、editor、max_versions；坏配置退出 2。

## 12. 校验、损坏与恢复

```sh
"$BAKON" --store ./store verify
"$BAKON" --store ./store verify --size
rm store/index.json
"$BAKON" --store ./store log ./trim-renamed.txt
"$BAKON" --store ./store verify --fix
test -f store/index.json && echo "index_rebuilt"
```

预期：`log` 在没有索引时仍能工作；`verify --fix` 重建索引；随后 `verify` 返回 0。

对象损坏：

```sh
obj=$(find store/objects -type f | head -1)
cp "$obj" "$obj.bak"
printf 'broken' > "$obj"
"$BAKON" --store ./store verify; echo "corrupt_exit=$?"
cp "$obj.bak" "$obj"
"$BAKON" --store ./store verify
```

预期：损坏时退出 6，指出文件/版本；恢复对象后再次一致。

崩溃恢复（仅 debug）：

```sh
printf 'one\n' > crash.txt
printf '#!/bin/sh\nprintf "two\\n" > "$1"\n' > two.sh
chmod +x two.sh
EDITOR="$T/two.sh" "$BAKON" --store ./store edit ./crash.txt

# 找到一个会留下 pending 的崩溃点
for n in $(seq 1 15); do
  BAKON_TEST_CRASH_AFTER=$n "$BAKON" --store ./store revert ./crash.txt 0 >/dev/null 2>&1
  code=$?
  if [ "$code" = 137 ] && [ -f store/.pending ] && grep -q '"pending_apply": true' store/files/*.json; then
    echo "pending_at=$n"; break
  fi
done

"$BAKON" --store ./store verify; echo "pending_verify_exit=$?"
printf 'external-edit\n' > crash.txt
"$BAKON" --store ./store verify --fix; echo "fix_exit=$?"
cat crash.txt
"$BAKON" --store ./store --format plain log ./crash.txt
```

预期：

- 找到 pending 后，普通 `verify` 退出 6 并报告 pending。
- `verify --fix` 退出 0，目标写回为 pending 内容。
- 日志出现 `salvage`（`external-edit`）和新的 `revert`；外部编辑没有丢失。

## 13. 并发

准备两个编辑器，分别等待同一个栅栏文件：

```sh
printf '#!/bin/sh\nwhile [ ! -f "$SYNC" ]; do sleep 0.01; done\nprintf "A\\n" > "$1"\n' > editor-a.sh
printf '#!/bin/sh\nwhile [ ! -f "$SYNC" ]; do sleep 0.01; done\nprintf "B\\n" > "$1"\n' > editor-b.sh
chmod +x editor-a.sh editor-b.sh
printf 'base\n' > race.txt
EDITOR="$T/noop.sh" "$BAKON" --store ./store edit ./race.txt
export SYNC="$T/go"
( EDITOR="$T/editor-a.sh" "$BAKON" --store ./store edit ./race.txt; echo "a=$?" ) &
( EDITOR="$T/editor-b.sh" "$BAKON" --store ./store edit ./race.txt; echo "b=$?" ) &
sleep 0.2
touch "$SYNC"
wait
"$BAKON" --store ./store --format plain log ./race.txt
```

预期：一个进程成功退出 0，另一个因 head 变化退出 4；历史中只多一个成功提交，磁盘上的编辑结果保留。

## 14. 锁与退出码

```sh
"$BAKON" --store ./nope log ./missing.txt; echo "missing_store_file_exit=$?"
"$BAKON" --store ./store show ./missing.txt 0; echo "unmanaged_exit=$?"
"$BAKON" --store ./store --format json version
"$BAKON" --store ./store verify --format json
```

预期：未纳管/缺失退出 3；`version` 与 `verify` 的 JSON 可解析。

## 15. 备份与恢复

```sh
cp -a store store.backup
"$BAKON" --store ./store.backup verify
printf 'after-copy\n' > trim-renamed.txt
EDITOR="$T/noop.sh" "$BAKON" --store ./store.backup edit ./trim-renamed.txt
"$BAKON" --store ./store.backup log ./trim-renamed.txt
```

预期：副本独立可用，`verify` 一致；对副本的编辑不影响原存储。

## 16. Release 产物

从 GitHub Actions 的 Release 运行中下载压缩包，解压后：

```sh
tar -xzf bakon_v1.0.0_linux-amd64.tar.gz
./bakon_v1.0.0_linux-amd64/bakon version
mkdir release-test && cd release-test
../bakon_v1.0.0_linux-amd64/bakon --store ./store init
printf 'release\n' > note.txt
../bakon_v1.0.0_linux-amd64/bakon --store ./store edit ./note.txt
../bakon_v1.0.0_linux-amd64/bakon --store ./store verify
```

预期：产物可执行，版本正确，`init`/`edit`/`verify` 正常。Windows 使用 `.zip` 中的 `bakon.exe`。

## 17. 平台补充

- **Windows**：确认 `bakon --store C:\bakon-test init`、`edit`、`revert` 可用；检查 `cmd /C` 下编辑器路径含空格；确认 `C:\Foo` 与 `c:\foo` 只产生一个清单。
- **macOS**：重复第 6 节符号链接用例；确认 `/tmp` 等系统符号链接路径不会误伤普通 `..` 输入。
- **Termux**：使用 linux arm64 产物，确认 `edit` 的编辑器与钩子可执行。

## 18. 清理

```sh
cd /
rm -rf "$T"
```

## 19. 记录表

| 编号 | 用例 | 结果 | 备注 |
|---|---|---|---|
| 1 | 构建与 `version` |  |  |
| 2 | `init` 与归属 |  |  |
| 3 | 纳管/历史/无改动 |  |  |
| 4 | `show`/`dump`/`diff` |  |  |
| 5 | `revert`/分歧/`--force` |  |  |
| 6 | 符号链接/`..` |  |  |
| 7 | 裁剪/对象回收 |  |  |
| 8 | `mv`/身份 |  |  |
| 9 | `ls`/`path`/`store`/JSON |  |  |
| 10 | 钩子成功/失败/递归 |  |  |
| 11 | 配置 |  |  |
| 12 | `verify`/损坏/崩溃恢复 |  |  |
| 13 | 并发 |  |  |
| 14 | 退出码 |  |  |
| 15 | 备份/恢复 |  |  |
| 16 | Release 产物 |  |  |
| 17 | 平台补充 |  |  |

# OPS__manual-testing

这是一次“以用户身份使用 bakon”的完整走查：从安装开始，管理一个演示配置文件，经历日常编辑、查看、回退、钩子、裁剪、改名、校验、备份和清理。按顺序执行即可；每一步都给出了应当看到的结果。

演示在独立目录中进行，不会碰真实的系统配置。

## 1. 准备

先安装或构建 bakon，并让 `bakon` 在 `PATH` 中：

```sh
cargo build --release -p bakon
export PATH="$PWD/target/release:$PATH"
```

创建演示环境：

```sh
export DEMO="$HOME/bakon-demo"
export BAKON_HOME="$DEMO/state"
mkdir -p "$BAKON_HOME" "$DEMO/files"
cd "$DEMO/files"
```

`BAKON_HOME` 让配置和存储在演示目录中，不污染你日常使用的 `~/.bakon`。

确认版本和命令：

```sh
bakon version
bakon --help
```

应当看到版本号、目标平台，以及 `init`、`edit`、`log`、`diff`、`revert`、`prune`、`hook`、`verify` 等命令。

## 2. 第一次使用

初始化存储：

```sh
bakon init
```

看到 `initialized .../bakon-demo/state/store`。再执行一次：

```sh
bakon init
```

看到 `already initialized`，不会重复创建。

创建一个演示配置：

```sh
cat > nginx.conf <<'EOF'
server {
    listen 80;
    server_name example.test;
    root /var/www/html;
}
EOF
```

第一次编辑并纳管：

```sh
export EDITOR=vi
bakon edit ./nginx.conf
```

编辑器打开后，把 `listen 80;` 改成 `listen 8080;`，保存退出。

如果不想改 `$EDITOR`，也可以在命令末尾指定编辑器：

```sh
bakon edit ./nginx.conf --editor nano
```

应当看到：

```text
bakon: tracked .../nginx-demo/files/nginx.conf (baseline version 0)
bakon: saved version 1 of .../nginx.conf
```

查看历史：

```sh
bakon log ./nginx.conf
```

应当有两行：

```text
  ver  time                  size  source
    1  ...                    ...  edit
    0  ...                    ...  baseline
```

新版本在前，版本号只增不减。

## 3. 日常查看与比较

查看版本 0 的原始内容：

```sh
bakon show ./nginx.conf 0
```

看到的是修改前的 `listen 80;`。导出到文件：

```sh
bakon dump ./nginx.conf 0 > nginx.conf.v0
```

比较最近两个版本：

```sh
bakon diff ./nginx.conf
```

应当看到 `-    listen 80;` 和 `+    listen 8080;`。有差异时命令退出码为 1，这是正常的：

```sh
echo $?    # 1
```

再编辑一次，把 `root /var/www/html;` 改成 `root /srv/www;`：

```sh
bakon edit ./nginx.conf
bakon log ./nginx.conf
```

现在应当有版本 0、1、2。

如果打开编辑器后没有修改就退出，`bakon edit` 会提示 `no changes`，不会产生空版本。

## 4. 回退

把文件恢复到版本 0：

```sh
bakon revert ./nginx.conf 0
cat nginx.conf
```

看到最初的内容。再查看历史：

```sh
bakon log ./nginx.conf
```

版本 0 和 1、2 仍在，最新一条是 `revert`。回退不会删除历史。

### 手工改过文件之后

模拟“绕过 bakon 直接改文件”：

```sh
printf 'server { listen 9090; }\n' > nginx.conf
bakon revert ./nginx.conf 0
echo $?    # 4
```

bakon 拒绝覆盖，因为当前磁盘内容与记录的最新版本不同。

确认要覆盖时：

```sh
bakon revert ./nginx.conf 0 --force
bakon log ./nginx.conf
```

日志里会出现一条 `salvage`，保存你刚才手工写的内容；随后才是新的 `revert`。之前的编辑不会凭空丢失。

## 5. 钩子

让每次保存后执行一条命令。这里用写日志代替真实的 reload：

```sh
bakon hook set ./nginx.conf 'printf "reloaded %s\n" "$BAKON_VERSION" >> hook.log'
```

修改一次配置：

```sh
bakon edit ./nginx.conf
cat hook.log
```

应当看到新版本号。钩子在版本保存成功后执行。

钩子失败不会回滚版本：

```sh
bakon hook set ./nginx.conf 'exit 9'
bakon edit ./nginx.conf
echo $?    # 5
```

版本仍然保存；如果不想执行钩子，可以加 `--no-hook`。

最后清除钩子：

```sh
bakon hook unset ./nginx.conf
```

## 6. 保留数量与裁剪

默认保留 100 个版本。为了演示裁剪，先多改几次：

```sh
for n in 1 2 3 4; do
  printf 'server { listen %s; }\n' "$((8080 + n))" > nginx.conf
  bakon edit ./nginx.conf
done
bakon log ./nginx.conf
```

现在版本比较多。写入配置：

```sh
bakon config init
sed -i.bak 's/max_versions = 100/max_versions = 3/' "$BAKON_HOME/config.toml"
bakon config show
```

确认 `max_versions` 为 3。先看裁剪会做什么：

```sh
bakon prune --dry-run
```

再实际裁剪：

```sh
bakon prune
bakon log ./nginx.conf
```

只保留最新 3 个版本。被裁剪的版本不能恢复；版本号不会回收。

查看存储占用：

```sh
bakon store size
```

## 7. 改名

把记录中的路径改掉：

```sh
bakon mv ./nginx.conf ./nginx-main.conf
bakon log ./nginx-main.conf
```

`mv` 只改记录，不移动磁盘文件。要让磁盘文件也叫新名字，自己再执行：

```sh
mv nginx.conf nginx-main.conf
```

旧路径不再被管理：

```sh
bakon log ./nginx.conf
echo $?    # 3
```

如果重新创建并纳管旧路径：

```sh
printf 'server { listen 80; }\n' > nginx.conf
bakon edit ./nginx.conf
bakon path ./nginx.conf
bakon path ./nginx-main.conf
```

两个 `path` 输出的 id 不同，历史互不串线。

## 8. 列出与定位文件

```sh
bakon ls
bakon path ./nginx-main.conf
bakon store path
bakon store size
```

`ls` 列出所有被管理路径；`path` 显示路径、id 和清单位置；`store` 显示存储位置或占用。

如果要脚本处理：

```sh
bakon --format json ls
bakon --format json path ./nginx-main.conf
bakon --format json store size
```

## 9. 一致性检查

```sh
bakon verify
```

应当显示 `store is consistent`。

模拟索引丢失：

```sh
rm "$BAKON_HOME/store/index.json"
bakon log ./nginx-main.conf
```

`log` 仍然可用，因为索引只是缓存。重建索引：

```sh
bakon verify --fix
bakon verify
```

现在恢复一致。

如果 `verify` 报告对象缺失或清单损坏，先备份整个 `$BAKON_HOME/store`，再按提示处理；不要直接删除存储目录。

## 10. 备份与恢复

复制整个存储目录就是备份：

```sh
cp -a "$BAKON_HOME/store" "$BAKON_HOME/store.backup"
bakon --store "$BAKON_HOME/store.backup" verify
```

恢复时，把备份目录复制回原位置，或直接用 `--store` 指向备份：

```sh
bakon --store "$BAKON_HOME/store.backup" log ./nginx-main.conf
```

备份目录可以独立使用，不影响原存储。

## 11. 出错时怎么看

- 文件没纳管或版本不存在：退出 3。
- 文件被外部修改，回退被拒绝：退出 4；用 `--force` 先记 `salvage`。
- 钩子失败：退出 5；版本已保存。
- 存储损坏或 `verify` 不一致：退出 6；先备份再按提示处理。
- 版本已记录但写回失败：退出 7；修复权限后重试。
- 权限或普通 IO 错误：退出 8。
- 版本已提交且可见，但父目录 `fsync` 失败、持久性未确认：退出 9；不要盲目重试，先用 `log`/`verify` 确认可见状态并备份。

诊断信息在 `stderr`；`stdout` 只放数据。

## 12. 清理演示环境

```sh
cd /
rm -rf "$DEMO"
```

删除后不影响真实配置；演示中的 `BAKON_HOME` 只指向这个目录。

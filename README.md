# bakon 使用说明

bakon 管理单个文件的版本历史：你用习惯的编辑器原地修改文件，bakon 在每次保存后记录一个新版本。版本号是递增整数，回退不会删除旧版本，而是再记一个新版本。

几个会反复出现的词：

- **纳管**：第一次让 bakon 管理某个文件，并把当时的内容记为版本 0。
- **版本**：某一次保存后的文件内容，用整数编号，0 是纳管时的基线。
- **存储**：bakon 保存历史副本的目录，默认在 `~/.bakon/store`。
- **钩子**：版本保存后自动执行的一条命令。
- **回退**：把文件内容恢复成某个旧版本，并追加一条新版本。

## 按目标查阅

- 安装 bakon
- 让编辑文件像平时一样顺手
- 第一次管理一个文件
- 查看历史、比较和导出旧版本
- 回退到旧版本
- 处理“文件已经被别的东西改过”
- 保存后自动执行命令
- 控制保留多少个版本、清理空间
- 给文件改名
- 查看所有被管理的文件和存储位置
- 修改编辑器、存储位置或保留数量
- 检查、备份和恢复存储
- 遇到错误时怎么办

## 安装

### 前提

- Linux、macOS 或 Windows。
- 如果从源码安装，需要 Rust；预先编译好的压缩包不需要 Rust。
- 你有一个可写的目录来放可执行文件，例如 `~/.local/bin`。

### 步骤

1. 从 Releases 下载与平台对应的压缩包。Linux/macOS 是 `.tar.gz`，Windows 是 `.zip`。

2. 解压并把可执行文件放到 `PATH` 中的目录。Linux 示例：

   ```sh
   tar -xzf bakon_*_linux-amd64.tar.gz
   mkdir -p "$HOME/.local/bin"
   install -m 755 bakon_*_linux-amd64/bakon "$HOME/.local/bin/bakon"
   ```

   做完后执行：

   ```sh
   "$HOME/.local/bin/bakon" version
   ```

   应当看到版本号和目标平台。

3. 让当前 shell 找到它：

   ```sh
   export PATH="$HOME/.local/bin:$PATH"
   ```

   然后执行：

   ```sh
   bakon version
   ```

   如果仍然提示 `command not found`，说明 `PATH` 没有包含上面的目录。

4. 把 `export` 写进 shell 启动文件，避免每次重开终端都再设置：

   ```sh
   # bash
   echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"

   # zsh
   echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.zshrc"
   ```

   重新打开终端，或执行 `source ~/.bashrc`（zsh 用 `~/.zshrc`）。

5. 也可以直接从源码安装。在仓库根目录执行：

   ```sh
   cargo install --path crates/bakon
   ```

   可执行文件通常在 `~/.cargo/bin/bakon`，该目录一般已在 `PATH` 中。

6. 如果希望放进系统目录，Linux/macOS 的常规位置是 `/usr/local/bin`：

   ```sh
   sudo install -m 755 target/release/bakon /usr/local/bin/bakon
   ```

   这需要 sudo 权限。不建议直接写 `/bin`，它通常是系统管理的目录。

Windows：解压 `.zip`，把 `bakon.exe` 所在目录加入“环境变量 → Path”，重新打开终端后执行 `bakon version`。

### 常见偏差

- `bakon: command not found`：可执行文件不在 `PATH`。用绝对路径执行一次，确认文件存在，再把目录加进 `PATH`。
- `Permission denied`：可执行文件没有执行权限。执行 `chmod +x`，或用 `install -m 755` 重新安装。
- 安装后仍运行旧版本：`PATH` 中有另一个 `bakon`。执行 `command -v bakon` 查看实际使用的是哪一个。

### 完成后

`bakon version` 能输出结果；`bakon --help` 能列出命令。

## 让编辑文件像平时一样顺手

### 前提

- 已完成安装，`bakon` 在 `PATH` 中。
- 你平时使用某个编辑器，例如 `vim`、`nano` 或 `code`。

### 步骤

1. 给 `bakon edit` 加一个短别名：

   ```sh
   alias be='bakon edit'
   ```

2. 以后编辑文件：

   ```sh
   be /etc/hosts
   ```

   这等价于 `bakon edit /etc/hosts`。

3. 如果想把某个编辑器固定成默认方式，可以再加一个带 `--editor` 的别名：

   ```sh
   alias vbe='bakon edit --editor vim'
   alias nbe='bakon edit --editor nano'
   ```

   之后 `vbe /etc/hosts` 用 vim，`nbe /etc/hosts` 用 nano。

4. 把别名写进 shell 启动文件：

   ```sh
   echo "alias be='bakon edit'" >> "$HOME/.bashrc"
   ```

### 常见偏差

- 不要用 `alias vim='bakon edit'` 覆盖真实编辑器。这会让所有需要 vim 的程序都走 bakon，语义容易混乱。用 `be`、`vbe` 这类新名字。
- 别名只对当前 shell 生效；写进启动文件后新开终端才自动生效。

### 完成后

`be <文件>` 直接进入 bakon 的编辑流程。

## 第一次管理一个文件

### 前提

- `bakon` 已可用。
- 目标文件存在，并且你有读写权限。
- 目标是一个普通文件，或指向普通文件的符号链接。

### 步骤

1. 初始化存储。可以显式执行，也可以在第一次 `edit` 时自动完成；显式执行更清楚：

   ```sh
   bakon init
   ```

   看到 `initialized`。再执行一次会看到 `already initialized`，不会重复创建。

2. 打开文件：

   ```sh
   bakon edit /etc/hosts
   ```

   第一次会看到：

   ```text
   bakon: tracked /etc/hosts (baseline version 0)
   ```

   然后编辑器打开。此时版本 0 已经保存为编辑前的内容。

3. 在编辑器里做修改，保存并退出。

   看到：

   ```text
   bakon: saved version 1 of /etc/hosts
   ```

4. 确认历史：

   ```sh
   bakon log /etc/hosts
   ```

   应当看到版本 1（来源 `edit`）和版本 0（来源 `baseline`）。

5. 如果打开编辑器后没有修改就退出，看到的是：

   ```text
   bakon: no changes to /etc/hosts
   ```

   不会产生空版本。

### 指定编辑器

临时换编辑器，不改环境变量：

```sh
bakon edit /etc/hosts --editor nano
```

`--editor` 只影响这一次。也可以设置环境变量：

```sh
EDITOR=nano bakon edit /etc/hosts
```

选择顺序是：`--editor`，然后 `$VISUAL`，然后 `$EDITOR`，然后配置里的 `editor`，最后 `vi`。

### 常见偏差

- 文件不存在：先创建文件，或确认路径没有拼错。
- 目标是目录、设备或管道：bakon 会拒绝；它只管理普通文件。
- 编辑器打不开：用 `--editor` 明确指定编辑器，或检查 `$EDITOR` 是否指向不存在的命令。
- 没有权限：启动编辑器可能成功，但保存或写回失败；检查文件权限。

### 完成后

文件已被纳管，`bakon log <文件>` 能看到至少版本 0；如果改过内容，还能看到版本 1。

## 查看历史、比较和导出旧版本

### 前提

- 文件已被 bakon 纳管。

### 步骤

1. 查看历史：

   ```sh
   bakon log ./app.conf
   ```

   版本从新到旧排列。`source` 说明这条版本来自首次纳管、普通编辑、回退还是保护性保存（`salvage`）。

2. 查看某个版本的原始内容：

   ```sh
   bakon show ./app.conf 0
   ```

   内容直接写到终端。`bakon dump ./app.conf 0` 效果相同，适合管道。

3. 把旧版本导出为文件：

   ```sh
   bakon show ./app.conf 0 > app.conf.v0
   ```

   重定向由 shell 完成，bakon 不会额外添加换行。

4. 比较最近两个版本：

   ```sh
   bakon diff ./app.conf
   ```

   看到 `-` 开头的旧行和 `+` 开头的新行。

5. 比较指定两个版本：

   ```sh
   bakon diff ./app.conf 0 1
   ```

6. 有差异时，`diff` 的退出码是 1：

   ```sh
   bakon diff ./app.conf
   echo $?
   ```

   这是“发现差异”，不是错误。没有差异时退出 0；版本不存在时退出 3。

### 常见偏差

- `diff` 返回 1 就以为命令失败：这是正常的差异结果，脚本里要单独处理。
- 只有一个版本时比较最近两版，会提示没有上一版；先再保存一次。
- `show` 输出二进制内容时，终端可能显示乱码；用重定向保存到文件。
- 版本号被裁剪后，`show` 旧版本会提示已被裁剪；裁剪不可恢复。

### 完成后

你能看到版本列表、导出任意仍在保留范围内的版本，并区分“有差异”和“命令出错”。

## 回退到旧版本

### 前提

- 文件已被纳管。
- 目标版本仍在保留范围内。

### 步骤

1. 查看历史，确认要回到哪个版本号：

   ```sh
   bakon log ./app.conf
   ```

2. 执行回退：

   ```sh
   bakon revert ./app.conf 0
   ```

   看到：

   ```text
   bakon: reverted ./app.conf to version 0 as version N
   ```

3. 查看文件内容，确认已经是版本 0 的内容。

4. 查看历史：

   ```sh
   bakon log ./app.conf
   ```

   最新一条是 `revert`，旧版本仍然在。回退不是删除历史。

### 常见偏差

- 当前磁盘内容与最新版本不一致：默认拒绝，退出 4。确认要覆盖时加 `--force`，它会先把当前内容记为 `salvage`，再回退。
- 目标版本已被裁剪：退出 3。裁剪掉的版本不能恢复；从备份或其他副本取回。
- 文件当前不存在：`revert` 会重新创建它。
- 目标路径是符号链接：bakon 写回真实目标，并保留链接本身。

### 完成后

文件内容等于目标版本；历史多了一条 `revert`，旧版本仍可查看。

## 处理“文件已经被别的东西改过”

### 前提

- 文件已被纳管。
- 你或其他程序绕过 bakon 修改了磁盘文件。

### 步骤

1. 再执行一次编辑：

   ```sh
   bakon edit ./app.conf
   ```

   编辑器打开的是当前磁盘内容。保存后，这次内容会作为新版本记录下来，不会丢失。

2. 如果你直接执行回退：

   ```sh
   bakon revert ./app.conf 0
   ```

   当前内容与记录的最新版本不同，bakon 会拒绝并退出 4。

3. 确认要丢掉当前内容再回退：

   ```sh
   bakon revert ./app.conf 0 --force
   ```

   当前内容会先记为一条 `salvage` 版本，然后才应用目标版本。

4. 查看历史，确认真实内容还在：

   ```sh
   bakon log ./app.conf
   ```

   `salvage` 那条就是覆盖前的内容。

### 常见偏差

- 不想保留当前内容时也不要直接手工删除；`--force` 的 `salvage` 是最省事的恢复点。
- `salvage` 也会占用保留数量，之后可能被裁剪。

### 完成后

要么当前内容成为一个可查看的版本，要么它被 `salvage` 保存；没有内容在没有记录的情况下消失。

## 保存后自动执行命令

### 前提

- 文件已被纳管。
- 钩子命令在 bakon 当前的 shell 环境中可执行。

### 步骤

1. 设置钩子：

   ```sh
   bakon hook set ./nginx.conf 'systemctl reload nginx'
   ```

   如果只是演示，可以写日志：

   ```sh
   bakon hook set ./nginx.conf 'echo reloaded >> hook.log'
   ```

2. 编辑文件：

   ```sh
   bakon edit ./nginx.conf
   ```

   版本保存成功后，钩子执行。

3. 查看钩子：

   ```sh
   bakon hook show ./nginx.conf
   ```

4. 临时跳过钩子：

   ```sh
   bakon edit ./nginx.conf --no-hook
   ```

5. 删除钩子：

   ```sh
   bakon hook unset ./nginx.conf
   ```

钩子会收到这些环境变量：

```text
BAKON_FILE       目标文件绝对路径
BAKON_VERSION    新版本号
BAKON_SOURCE     edit 或 revert
BAKON_PREV       上一版本号；首个版本为空
BAKON_HOOK=1     表示当前在钩子中，防止递归
```

### 常见偏差

- 钩子失败：版本已经保存，但命令退出 5。先修钩子命令，版本不会丢。
- 钩子死循环：钩子里又调用 bakon 编辑同一文件时，子进程会看到 `BAKON_HOOK=1` 并跳过钩子；不要依赖它做无限递归。
- 钩子没有执行：检查是否用了 `--no-hook`，或当前环境是否设置了 `BAKON_HOOK`。
- 钩子只执行一次；如果执行期间进程中断，不会自动重试。

### 完成后

每次成功保存后，钩子按预期执行；失败时你能从退出码 5 和 stderr 看出原因。

## 控制保留多少个版本、清理空间

### 前提

- 文件已被纳管。
- 你知道裁剪会永久删除超出保留数量的旧版本。

### 步骤

1. 查看当前配置：

   ```sh
   bakon config show
   ```

2. 初始化配置文件（如果还没有）：

   ```sh
   bakon config init
   ```

3. 编辑配置文件，把 `max_versions` 改成想要的数量。例如保留 3 个：

   ```toml
   [retention]
   max_versions = 3
   ```

   `0` 表示不限制。

4. 查看配置是否生效：

   ```sh
   bakon config show
   ```

5. 先预演裁剪：

   ```sh
   bakon prune --dry-run
   ```

   只报告会删哪些版本，不改变文件。

6. 实际裁剪：

   ```sh
   bakon prune
   ```

   也可以只裁剪一个文件：

   ```sh
   bakon prune ./nginx.conf
   ```

7. 查看结果：

   ```sh
   bakon log ./nginx.conf
   bakon store size
   ```

   版本数不超过上限；存储占用随裁剪下降。

### 常见偏差

- 裁剪不可恢复：确认保留数量再执行。需要保留的内容先 `show` 导出。
- 版本号不会因为裁剪回收；裁剪后 `revert` 到旧号会提示已裁剪。
- `max_versions = 0` 表示无限保留，不是“删到 0 个”。
- 只改配置不执行 `prune` 时，已有文件不会立刻按新上限收缩；下次提交或手动 `prune` 才生效。

### 完成后

`bakon log` 只剩允许保留的版本；`bakon verify` 仍显示一致。

## 给文件改名

### 前提

- 文件已被纳管。
- 新路径当前没有被另一个被管理文件占用。

### 步骤

1. 只改 bakon 的记录：

   ```sh
   bakon mv ./old.conf ./new.conf
   ```

   磁盘上的 `old.conf` 不会移动。

2. 如果你也希望磁盘文件改名，自己执行：

   ```sh
   mv old.conf new.conf
   ```

3. 用新路径查看历史：

   ```sh
   bakon log ./new.conf
   ```

4. 旧路径不再被管理：

   ```sh
   bakon log ./old.conf
   ```

   会提示未纳管，退出 3。

5. 如果重新创建旧路径并再次纳管，它会有新的身份和历史：

   ```sh
   printf 'new file\n' > old.conf
   bakon edit ./old.conf
   bakon path ./old.conf
   bakon path ./new.conf
   ```

   两个 `path` 输出的 id 不同。

### 常见偏差

- `mv` 后直接编辑新路径，但磁盘文件还没改名：会提示文件不存在。先移动磁盘文件，或先 `edit` 旧路径再 `mv`。
- 新路径已被另一个被管理文件占用：命令拒绝，避免两个文件共享历史。
- 只改记录不会改变文件权限或内容。

### 完成后

记录中的路径是新路径；旧路径不再有历史；重新纳管的旧路径是独立文件。

## 查看所有被管理的文件和存储位置

### 前提

- 存储已初始化。

### 步骤

1. 列出所有被管理路径：

   ```sh
   bakon ls
   ```

2. 查看一个文件的路径、id 和清单文件位置：

   ```sh
   bakon path ./app.conf
   ```

3. 查看存储根目录：

   ```sh
   bakon store path
   ```

4. 查看磁盘占用：

   ```sh
   bakon store size
   ```

5. 需要脚本处理时用 JSON：

   ```sh
   bakon --format json ls
   bakon --format json path ./app.conf
   bakon --format json store size
   ```

### 常见偏差

- `ls` 没有输出：当前存储里还没有被管理的文件。
- `path` 提示未纳管：路径拼写、当前目录或 `--store` 指向的存储不对。
- 多个路径看起来相同：在大小写不敏感的系统上，bakon 会把它们视为同一个路径；用 `bakon path` 确认 id。

### 完成后

你知道哪些文件被管理、它们的历史在哪里，以及存储占了多少空间。

## 修改编辑器、存储位置或保留数量

### 前提

- 你知道要改的是“全局配置”还是“这一次命令”。

### 步骤

1. 写入默认配置：

   ```sh
   bakon config init
   ```

   如果文件已存在，命令不会覆盖。

2. 查看配置文件位置：

   ```sh
   bakon config show
   ```

3. 编辑配置文件中的字段：

   ```toml
   store = "/absolute/path/to/store"
   editor = "nano"

   [retention]
   max_versions = 100
   ```

4. 检查生效值：

   ```sh
   bakon config show
   ```

5. 临时覆盖存储位置：

   ```sh
   bakon --store /tmp/other-store log ./app.conf
   ```

6. 临时覆盖编辑器：

   ```sh
   bakon edit ./app.conf --editor nano
   ```

配置和存储的查找顺序：

- 存储：`--store` → `BAKON_STORE` → 配置 `store` → `$BAKON_HOME/store` → `~/.bakon/store`。
- 配置：`--config` → `BAKON_CONFIG` → `$BAKON_HOME/config.toml` → `$XDG_CONFIG_HOME/bakon/config.toml` → `~/.config/bakon/config.toml`。
- 编辑器：`--editor` → `$VISUAL` → `$EDITOR` → 配置 `editor` → `vi`。

### 常见偏差

- 配置里的 `store` 用相对路径：不同目录执行会指向不同位置；建议写绝对路径。
- 修改 `store` 后看不到旧历史：新存储是另一个目录；把旧存储路径换回来。
- 修改 `editor` 后没生效：`$VISUAL`/`$EDITOR` 的优先级更高；用 `--editor` 可以临时确认。

### 完成后

`bakon config show` 显示你期望的存储、编辑器和保留数量；后续命令按这些值执行。

## 检查、备份和恢复存储

### 前提

- 你知道存储目录的位置（`bakon store path`）。

### 步骤

1. 做一次一致性检查：

   ```sh
   bakon verify
   ```

   一致时输出 `store is consistent`，退出 0。

2. 查看占用明细：

   ```sh
   bakon verify --size
   ```

3. 如果 `verify` 报索引或残留问题，先备份，再修复：

   ```sh
   cp -a "$(bakon store path)" "$(bakon store path).backup"
   bakon verify --fix
   bakon verify
   ```

4. 备份存储：复制整个存储目录即可。

5. 从备份恢复：把备份目录复制回原位置，或用 `--store` 指向备份：

   ```sh
   bakon --store /path/to/store.backup verify
   ```

6. 如果索引文件丢失，不需要手工重建；`bakon log` 仍能工作，`bakon verify --fix` 会重建索引。

### 回退过程中被中断

如果回退写文件时进程被强制终止，下次执行任意写命令会继续完成写回。若你在这期间手工改过目标文件，bakon 会先把这份内容记为 `salvage`，再完成回退。执行：

```sh
bakon verify --fix
bakon log ./app.conf
```

确认 `salvage` 或 `revert` 记录存在。

### 常见偏差

- `verify` 退出 6 不代表所有数据都没了；先备份，再按提示处理，不要直接删除存储目录。
- 备份时只复制 `files/` 或 `objects/` 的一部分，会得到不完整的历史；复制整个存储目录。
- 不要在 bakon 运行时手工修改存储里的清单或对象；需要修复时用 `verify --fix`。

### 完成后

`bakon verify` 一致；你有一个可独立使用的存储副本。

## 遇到错误时怎么办

| 现象 | 含义 | 处理 |
|---|---|---|
| 退出 3 | 文件未纳管、版本不存在或已被裁剪 | 用 `bakon log` 确认路径和版本号 |
| 退出 4 | 锁超时、并发冲突，或文件被外部修改 | 稍后重试；确认要覆盖时用 `revert --force` |
| 退出 5 | 版本已保存，但钩子或编辑器失败 | 修命令或编辑器；版本已经保存 |
| 退出 6 | 存储损坏、不一致、不是 bakon 存储或格式不兼容 | 先备份整个存储，再执行 `verify --fix` 或按提示处理 |
| 退出 7 | 版本已记录，但目标文件写回失败 | 检查权限、只读挂载或磁盘空间，然后重试 |
| 退出 8 | 权限或普通 IO 错误 | 检查目录权限和磁盘状态 |
| 退出 9 | 版本已写入并可见，但父目录 `fsync` 失败，持久性未确认 | 不要盲目重试；先用 `bakon log` / `bakon verify` 确认可见状态并备份；断电可能丢失该版本 |
| `path is not valid UTF-8` | 目标路径不是 UTF-8 | 使用 UTF-8 路径；bakon 把路径存为文本 |
| `path traverses a symbolic link before '..'` | 路径中的 `..` 前面经过符号链接，实际目标不确定 | 使用解析后的绝对路径，去掉 `..` |

脚本只应依赖退出码和 `--format json` 输出，不要依赖错误消息文本。

## 停止使用或卸载

1. 确认不再需要历史后，先备份存储目录。
2. 删除可执行文件，例如：

   ```sh
   rm "$HOME/.local/bin/bakon"
   ```

3. 删除存储和配置：

   ```sh
   rm -rf "$HOME/.bakon"
   rm -rf "$HOME/.config/bakon"
   ```

   这会永久删除历史，执行前确认路径。

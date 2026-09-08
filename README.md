# tapd-cli

`tapd-cli` 是一个面向人工操作和 AI Agent 编排的 TAPD Open API 命令行工具。它提供稳定的 JSON 输出、按 profile 隔离的系统凭证、写操作 dry-run、附件与正文图片回读校验，以及基于实际工作流图的安全状态流转。

项目采用三层结构：

```text
tapd-cli             命令解析、配置、凭证、输出和内置 Agent Skill
├── tapd-client      与组织无关的 TAPD Open API 客户端
└── tapd-domain-demo 中性的领域组合示例
```

`tapd-domain-demo` 只用于演示如何在通用客户端之上固定 profile、空间、类别、工作项类型和标题模板。真实组织可以在私有 crate 中实现自己的配置和流程，而不修改 `tapd-client`。

## 安装

从 GitHub 安装：

```bash
cargo install --git https://github.com/1998y12/tapd-cli tapd-cli
```

从源码构建：

```bash
cargo build --release
```

不包含领域示例命令的纯通用版本：

```bash
cargo build --release -p tapd-cli --no-default-features
```

## 配置与登录

生成配置示例：

```bash
tapd-cli config init --stdout
```

默认配置路径是 `~/.config/tapd-cli/config.toml`，也可以使用 `--config` 或 `TAPD_CLI_CONFIG` 指定。

普通配置写入 TOML，访问令牌写入系统密钥库：

```bash
tapd-cli --profile demo auth login
tapd-cli --profile demo auth status
tapd-cli --profile demo auth test
```

macOS 使用 Keychain，Windows 使用 Credential Manager，Linux 使用 Secret Service。密钥项的 service 是 `tapd-cli`，account 是 profile 名。非空 `token_env` 仍可用于 CI 或临时覆盖，并且优先于系统密钥库。

CLI 不提供明文 `--token` 参数。

## 通用命令

```text
tapd-cli auth ...
tapd-cli workspace ...
tapd-cli story ...
tapd-cli comment ...
tapd-cli attachment ...
tapd-cli image ...
tapd-cli workflow ...
tapd-cli metadata ...
tapd-cli url parse ...
```

查询需求，默认每页 30 条，单页最多 200 条：

```bash
tapd-cli story list
tapd-cli story list --limit 100 --page 2 --order 'modified desc'
tapd-cli story count --status STATUS
tapd-cli story get STORY_ID
```

创建和更新需求：

```bash
tapd-cli --dry-run story create \
  --name '业务标题' \
  --description-file ./description.md

tapd-cli story create \
  --name '业务标题' \
  --description-file ./description.md

tapd-cli story update STORY_ID --owner USER
```

安全流转到目标状态：

```bash
tapd-cli --dry-run story transition STORY_ID --to '目标状态'
tapd-cli story transition STORY_ID --to '目标状态'
```

命令会读取需求实际类别、当前状态和工作流图，计算合法路径，检查必填字段，并在每一步写入后回读确认。

## 正文图片

Markdown 或 HTML 正文使用显式占位符：

```markdown
![趋势图]({{tapd-image:trend}})
```

创建时映射到本地图片：

```bash
tapd-cli story create \
  --name '业务标题' \
  --description-file ./description.md \
  --image trend=./trend.png
```

CLI 会验证全部映射和本地文件，逐张上传图片，通过 `/files/get_image` 回读校验空间和路径，替换占位符，再写入需求正文。支持 PNG、GIF、JPG/JPEG 和 BMP；单次命令最多 20 张。

## Agent Skill

仓库提供一个主 Skill，并通过 `references/` 渐进加载各类命令知识。Skill 源文件与编译进二进制的内容来自同一目录，确保 CLI 与 Skill 版本一致。

使用当前二进制离线安装：

```bash
tapd-cli skill list
tapd-cli skill read
tapd-cli skill install
tapd-cli skill status
tapd-cli skill update
tapd-cli skill uninstall
```

`skill install` 默认自动检测 Codex、Claude Code 和 Cursor，并安装到用户级目录。也可以明确指定：

```bash
tapd-cli skill install --agent codex
tapd-cli skill install --agent claude-code
tapd-cli skill install --agent cursor
tapd-cli skill install --agent codex --scope project
tapd-cli skill install --dir /custom/skills
```

项目级安装使用标准共享目录 `.agents/skills/tapd-cli`。安装清单记录 CLI 版本和 bundle hash；`update` 只更新由本工具管理的安装，`uninstall` 只逐项删除内置文件，不递归删除用户增加的内容。

也可以通过 GitHub CLI 安装仓库中的同一份 Skill：

```bash
gh skill install 1998y12/tapd-cli tapd-cli \
  --agent codex \
  --scope user
```

## 领域扩展示例

先填写配置中的 `[organizations.demo.request]`，然后查看领域层生成的通用 TAPD 参数：

```bash
tapd-cli demo request plan \
  --subject 'Reporting' \
  --summary 'Add a monthly view'
```

实际提交：

```bash
tapd-cli demo request submit \
  --subject 'Reporting' \
  --summary 'Add a monthly view'
```

实现说明见 [`crates/tapd-domain-demo`](crates/tapd-domain-demo) 和 [`references/domain-extension.md`](skills/tapd-cli/references/domain-extension.md)。

## 安全约束

- token 不接受明文命令行参数、不写入配置文件、不打印到日志；
- 所有 TAPD ID 按字符串处理，避免长 ID 精度损失；
- 附件上传后回读并核对附件 ID、对象 ID、类型和文件名；
- 正文图片上传后回读并核对空间 ID 和图片路径；
- 状态流转基于服务器返回的状态图，不盲目修改状态字段；
- 工具不会主动向远端业务内容注入来源、运行编号或生成器声明；
- dry-run 只解析和验证，不向 TAPD 写入。

完整架构见 [docs/architecture.md](docs/architecture.md)，配置示例见 [examples/tapd-cli.example.toml](examples/tapd-cli.example.toml)。

## License

[MIT](LICENSE)

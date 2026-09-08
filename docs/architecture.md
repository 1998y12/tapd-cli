# tapd-cli 架构

## 设计目标

`tapd-cli` 将 TAPD 的通用协议能力、命令行交互和组织领域编排分开，使通用部分可以独立复用，私有流程可以单独演进。

首版通用范围包括认证、空间、需求、评论、附件、正文图片、工作流、需求元数据、URL 解析、JSON 输出和 dry-run。未使用的 TAPD 资源可以继续按相同模式扩展。

## Crate 分层

```text
tapd-cli（binary）
├── tapd-client（通用 Open API 客户端）
└── tapd-domain-demo（组织领域组合示例）
        └── tapd-client
```

`tapd-client` 不依赖 Clap，不理解任何组织规则，负责：

- Bearer 认证头和请求超时；
- GET、表单 POST 和 multipart POST；
- TAPD `status/info/data` 响应解析；
- 长 ID 的无损字符串转换；
- 附件和正文图片上传后的回读校验；
- 工作流图解析和合法路径规划；
- TAPD 页面 URL 构造与解析。

`tapd-cli` 负责参数解析、配置发现、系统凭证、内容转换、稳定 JSON 输出和命令编排。

`tapd-domain-demo` 展示如何在独立 crate 中定义配置、业务输入、参数计划和多调用服务。真实扩展只依赖 `tapd-client`，通过可选 feature 接入最终二进制。

## 配置和凭证

配置文件只保存非敏感内容。profile 可以固定 API 地址、网页地址、空间 ID、环境变量名和超时时间；`organizations` 表由相应领域 crate 自行解析。

凭证优先级：

1. profile 的非空 `token_env`；
2. 系统密钥库中 service=`tapd-cli`、account=`<profile>` 的条目。

环境变量适合 CI 和临时覆盖；日常使用通过 `auth login` 隐藏输入并保存到系统密钥库。

## 写操作

普通写操作先构造确定的参数集合。dry-run 返回方法、端点、参数和附加计划，不发送请求。

附件上传后调用附件列表接口，核对 ID、依赖对象、类型和文件名。正文图片上传后调用 `/files/get_image`，核对路径和空间 ID。状态流转先读取实际工作项类型与状态图，找到合法路径并检查必填字段，每一步写入后回读状态。

## 领域扩展

领域 crate 应当拥有：

- 自己的配置结构和校验；
- 可配置的稳定空间、类别、工作项类型、模板和状态标识；
- 业务输入类型；
- 从业务输入到通用 TAPD 参数的转换；
- 必要的多接口编排与回读验证；
- 确保远端内容仅包含预期业务信息的测试。

领域 crate 不应复制 HTTP、认证和响应解析逻辑，也不应把真实凭证或组织内部标识放入公开示例。

## Agent Skill

```text
skills/tapd-cli/
├── SKILL.md
└── references/
    ├── auth-and-config.md
    ├── workspaces-and-output.md
    ├── stories.md
    ├── comments.md
    ├── attachments-and-images.md
    ├── workflows-and-metadata.md
    └── domain-extension.md
```

主 Skill 只保存工具选择、凭证安全、写操作边界和 reference 路由。详细命令按主题渐进加载，避免每次把完整手册放入 Agent 上下文。

这些文本在编译时通过 `include_str!` 进入二进制。内置安装器支持用户级 Codex、Claude Code、Cursor，自定义目录，以及共享的项目级 `.agents/skills` 目录。清单文件保存版本、文件列表和 bundle hash，使 status、update、uninstall 可以识别并保护非本工具管理的安装。

## 测试

- 单元测试：配置优先级、凭证来源、内容转换、Skill bundle、工作流图和领域计划；
- HTTP 合约测试：认证头、响应 envelope、附件 multipart、图片 multipart 和写后回读；
- CLI 冒烟：帮助、通用版裁剪、dry-run、图片占位符和领域示例；
- 发布前同时运行完整 feature 与 `--no-default-features` 的测试和严格 Clippy。

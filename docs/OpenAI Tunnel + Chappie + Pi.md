# 把 ChatGPT 网页版接到本地开发环境：OpenAI Tunnel + Chappie + Pi 完整实践

ChatGPT 网页版本身运行在云端，它可以理解代码、分析文件、调用工具，但默认情况下，它并不能直接访问你电脑上的项目。

OpenAI 提供的 Secure MCP Tunnel，解决了这中间最关键的一段连接问题。

通过 Tunnel，可以让本地程序主动连接 OpenAI，再把 ChatGPT 发起的 MCP 工具调用转发到本地。再配合 Chappie 和 Pi，ChatGPT 就可以真正操作本地项目：

- 读取文件
- 修改代码
- 执行 Git
- 运行构建
- 跑测试
- 查看日志
- 读取图片
- 在云端与本地之间传输文件
- 同时管理多个本地项目

整个过程中不需要公网 IP，不需要暴露本地 HTTP 服务，也不需要配置路由器端口转发。

最终形成的链路是：

```text
ChatGPT 网页版
      │
      │ MCP Tool Call
      ▼
OpenAI Secure MCP Tunnel
      │
      ▼
   otunnel
      │
      │ stdio
      ▼
 chappie
      │
      │ session broker
      ▼
项目中的 Pi Session
      │
      ├── read
      ├── write
      ├── edit
      ├── bash
      ├── git
      ├── build
      └── test
```

这套架构最重要的一点，是：

> ChatGPT 负责思考和上下文，Pi 负责本地执行。

Pi 不需要再调用另一个大模型，也不需要维护一套和 ChatGPT 重复的上下文。

---

# 一、这套架构到底是如何工作的

先把几个组件分清楚。

## ChatGPT

ChatGPT 是真正的模型。

它负责：

```text
理解需求
分析代码
制定修改方案
决定调用什么工具
处理长上下文
读取图片
联网查询
代码审查
推理
```

ChatGPT 本身并不能直接看到：

```text
C:\work\project
D:\repos\project-a
/home/user/project
```

所以必须通过 MCP 工具访问本地。

---

## MCP

MCP 可以理解成一套标准化的“模型调用工具协议”。

ChatGPT 可以发出：

```text
read(path="src/main.rs")
```

或者：

```text
bash(command="git status")
```

本地 MCP Server 执行之后，把结果返回给 ChatGPT。

问题在于：

```text
ChatGPT 在云端
MCP Server 在你的电脑
```

二者不能直接通过 localhost 通信。

---

# 二、OpenAI Tunnel 解决了什么

传统方案通常需要：

```text
公网 IP
域名
HTTPS
证书
Nginx
端口转发
Cloudflare Tunnel
ngrok
鉴权
防火墙
```

Secure MCP Tunnel 的工作方式不同。

不是：

```text
OpenAI
  ↓
主动连接你的电脑
```

而是：

```text
你的电脑
  ↓
主动连接 OpenAI
```

本地的 tunnel client 会持续通过 HTTPS 请求 OpenAI：

```text
是否有新的 MCP 请求？
```

如果有，就把请求拉回来，交给本地 MCP Server。

执行完成以后，再把结果发回 OpenAI。

所以实际网络方向是：

```text
本地电脑
   │
   │ HTTPS 443 出站
   ▼
OpenAI Tunnel
```

一般不需要：

```text
Windows 防火墙开放入站端口
路由器端口转发
公网 IP
反向代理
```

---

# 三、Chappie 和 Pi 分别负责什么

当前版本里，Chappie Broker 和项目 Pi Session 是两个独立角色。

很多第一次配置的人最容易在这里混淆。

## Chappie：独立 MCP Server + Session Broker

Tunnel 配置里启动的是：

```bash
chappie
```

它不是项目开发进程，而是独立的 stdio MCP Server，同时在本机 `~/.chappie` 对应的 IPC 端点上维护 Session Broker。

链路类似：

```text
otunnel stdin/stdout
  ↓
chappie MCP Server
  ↓
本地 Chappie Broker
  ↓
已注册的 Pi Session
```

它本身不代表某一个具体项目，而是所有本地 Pi Session 的统一入口与路由层。

---

## Pi：真正的项目 Session

真正操作项目时，需要进入项目目录：

```powershell
cd D:\work\project-a
```

然后：

```powershell
pi --provider chappie --model chatgpt
```

这个 Pi 才代表：

```text
当前项目
当前工作目录
当前工具环境
当前终端
当前 Git 仓库
```

官方 Chappie 使用方式就是：

```bash
pi --provider chappie --model chatgpt
```



于是完整关系变成：

```text
ChatGPT
   │
   ▼
OpenAI Tunnel
   │
   ▼
otunnel
   │
   ▼
chappie
   │
   ├─────────────┬─────────────┐
   ▼             ▼             ▼
Pi Session A   Pi Session B   Pi Session C
ProjectA       ProjectB    ProjectC
```

---

# 四、环境准备

下面以 Windows 为例。

建议使用：

```text
Windows 10 / 11
PowerShell
nvm-windows
Node.js
Git
Rust / Cargo
Pi
Chappie
otunnel
```

---

# 五、创建 OpenAI Tunnel

打开：

```text
https://platform.openai.com/settings/organization/tunnels
```

登录与你 ChatGPT 工作空间对应的 OpenAI 账号。

创建 Tunnel。

建议填写：

```text
Name:
Chappie
```

创建后会得到：

```text
tunnel_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

例如：

```text
tunnel_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

保存这个 Tunnel ID。

---

# 六、创建 Tunnel API Key

打开：

```text
https://platform.openai.com/settings/organization/api-keys
```

新建 API Key。

名称：

```text
Tunnel
```

权限选择：

```text
Restricted
```

只给：

```text
Tunnels:
Read
Use
```

不要把长期运行的 Tunnel 配成完整 Admin 权限。

创建完成以后复制：

```text
sk-xxxxxxxx
```

---

# 七、把 API Key 存到文件

不要把密钥直接写进命令历史。

创建目录：

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\.chappie"
```

打开文件：

```powershell
notepad "$env:USERPROFILE\.chappie\tunnelkey.txt"
```

文件中只写：

```text
sk-xxxxxxxxxxxxxxxx
```

不要写：

```text
OPENAI_API_KEY=
```

也不要加引号。

---

# 八、安装 Node.js

首先检查：

```powershell
node -v
npm -v
```

当前 Chappie 包要求：

```text
Node >= 26
```

其 `package.json` 中当前声明：

```json
"engines": {
  "node": ">=26",
  "pnpm": ">=12"
}
```



如果正在使用：

```text
Node 24
```

即使 Pi 本身能够启动，Chappie 仍可能无法正常启动 MCP Server。

如果使用 nvm-windows：

```powershell
nvm list
```

安装一个 Node 26 版本：

```powershell
nvm install 26.8.2
```

切换：

```powershell
nvm use 26.8.2
```

确认：

```powershell
node -v
npm -v
```

还可以直接检查 Chappie 当前要求：

```powershell
npm view @zetaloop/chappie engines
```

这样比依赖旧文档更可靠。

---

# 九、安装 Pi

安装：

```powershell
npm install -g --ignore-scripts @earendil-works/pi-coding-agent
```

检查：

```powershell
pi --version
```

确认路径：

```powershell
where.exe pi
```

如果使用 nvm，需要注意：

> Node 不同版本的全局 npm 包通常彼此独立。

例如从 Node 24 切到 Node 26 后：

```powershell
pi --version
```

可能提示命令不存在。

重新执行一次：

```powershell
npm install -g --ignore-scripts @earendil-works/pi-coding-agent
```

即可。

---

# 十、安装 Chappie

执行：

```powershell
pi install npm:@zetaloop/chappie
```

确认：

```powershell
pi list
```

应该看到：

```text
@zetaloop/chappie
```

当前版本需要同时具备独立 Chappie CLI 和 Pi Extension。

安装独立 Broker CLI：

```powershell
npm install -g @zetaloop/chappie@1
```

检查 Broker CLI：

```powershell
chappie --version
```

同时确认 Pi Extension：

```powershell
pi list
```

应能看到 `npm:@zetaloop/chappie`。如果独立 CLI 与 Pi Extension 版本长期错位，可能出现 Pi 已选择 `chappie/chatgpt`，但 Session 始终无法注册到 Broker 的情况；此时先执行：

```powershell
pi update --extensions
```

再重新启动 Pi Session。

---

# 十一、安装 Rust 和 Cargo

检查：

```powershell
cargo --version
```

如果没有安装 Rust，先安装 Rust 工具链。

安装完成后重新打开 PowerShell：

```powershell
cargo --version
```

---

# 十二、安装 cargo-binstall

检查：

```powershell
cargo binstall --version
```

如果没有：

```powershell
cargo install cargo-binstall --locked
```

---

# 十三、安装 otunnel

执行：

```powershell
cargo binstall otunnel
```

确认：

```powershell
otunnel --version
```

以及：

```powershell
where.exe otunnel
```

---

# 十四、初始化 Tunnel Profile

准备两个变量：

```powershell
$TunnelId = "tunnel_你的TunnelID"
$KeyFile = "$env:USERPROFILE\.chappie\tunnelkey.txt"
```

初始化：

```powershell
otunnel init `
  --profile chappie `
  --tunnel-id $TunnelId `
  --control-plane-api-key-ref "file:$KeyFile" `
  --mcp-command "chappie"
```

这里：

```text
--profile chappie
```

表示配置名称。

```text
--tunnel-id
```

对应 OpenAI 创建的 Tunnel。

```text
--control-plane-api-key-ref
```

表示从本地文件读取 API Key。

```text
--mcp-command "chappie"
```

表示收到 MCP 请求之后，由：

```text
chappie
```

提供本地 MCP Server。

---

# 十五、运行 doctor

执行：

```powershell
otunnel doctor --profile chappie
```

正常应该看到：

```text
CHECK config_source             PASS
CHECK profile_load              PASS
CHECK tunnel_id                 PASS
CHECK control_plane_api_key     PASS
CHECK mcp_target                PASS
CHECK mcp_server_reachable      PASS
CHECK control_plane_connection  PASS
CHECK health_listener           PASS

RESULT ok
EXIT_CODE 0
```

---

# 十六、如何分析 doctor 输出

一个典型失败结果：

```text
CHECK config_source            PASS
CHECK profile_load             PASS
CHECK tunnel_id                PASS
CHECK control_plane_api_key    PASS
CHECK mcp_target               PASS chappie
CHECK mcp_server_reachable     FAIL
CHECK control_plane_connection PASS
```

这意味着：

```text
OpenAI Tunnel       正常
API Key             正常
Tunnel ID           正常
OpenAI 控制面       正常
本地 MCP 启动       异常
```

这时完全不需要重新创建 Tunnel。

重点检查：

```powershell
node -v
```

```powershell
npm view @zetaloop/chappie engines
```

```powershell
pi --version
```

```powershell
pi list
```

```powershell
chappie --version
(Get-Content "$HOME\.pi\agent\npm\node_modules\@zetaloop\chappie\package.json" -Raw | ConvertFrom-Json).version
```

最后直接运行：

```powershell
chappie
```

正常情况下：

```text
不会出现普通 Pi TUI
不会退出
会一直等待 stdin MCP 请求
```

使用：

```text
Ctrl+C
```

结束。

如果立即退出或者出现：

```text
module error
syntax error
package error
```

这才是真正需要解决的问题。

---

# 十七、正式启动 Tunnel

doctor 全部通过后：

```powershell
otunnel run --profile chappie
```

这个窗口长期保持运行。

它负责：

```text
OpenAI
   ↕
otunnel
   ↕
chappie
```

通常一台电脑只需要启动一个。

---

# 十八、ChatGPT 中创建 Chappie App

进入 ChatGPT。

打开 Developer Mode。

然后创建自定义 App。

名称：

```text
Chappie
```

连接类型：

```text
Tunnel
```

选择刚才创建的 Tunnel：

```text
tunnel_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

Authentication：

```text
None
```

然后扫描工具。

成功以后应该能够看到类似：

```text
init
sessions
tools
chat
call
read
bash
edit
write
transfer
```

Chappie 当前提供的核心 MCP 工具包括这些。

创建完成以后，可以根据需求配置工具权限。

---

# 十九、启动第一个项目

例如：

```text
D:\work\project-a
```

新开一个 PowerShell：

```powershell
cd D:\work\project-a
```

执行：

```powershell
pi --provider chappie --model chatgpt
```

这个 Pi 进程现在代表：

```text
项目：
D:\work\project-a

工作目录：
D:\work\project-a
```

只要它保持运行，Chappie Broker 就可以发现它。

---

# 二十、第一次在 ChatGPT 中连接项目

打开一个新的 ChatGPT 对话。

输入：

```text
@Chappie

先调用 sessions。

找到 cwd 为：

D:\work\project-a

的 Pi session。

确认只有一个匹配以后，
使用它的 sessionId 调用 init。

不要操作其他项目。
```

Chappie 的 `sessions` 会列出：

```text
sessionId
working directory
name
status
bindingCount
```



例如：

```text
Session

sessionId:
2af930...

cwd:
D:\work\project-a

status:
ready

bindingCount:
0
```

随后：

```text
init({
  sessionId: "2af930..."
})
```

当前 ChatGPT 对话就和这个项目建立了默认绑定。

---

# 二十一、完成第一次只读测试

绑定完成后不要立即改源码。

先执行：

```text
只做只读检查：

1. 告诉我当前工作目录
2. 执行 git status
3. 执行 git branch --show-current
4. 列出项目根目录文件
5. 读取 README.md 前 30 行

不要修改任何文件。
```

确认：

```text
cwd 正确
Git 仓库正确
Branch 正确
读取文件正确
```

---

# 二十二、测试写入

然后建立临时文件：

```text
在项目根目录创建：

chappie-test.txt

内容：

hello from ChatGPT MCP

创建后重新读取验证。

不要修改其他文件。
```

验证成功以后：

```text
删除 chappie-test.txt。
```

这样就完成：

```text
read
write
delete
```

基本验证。

---

# 二十三、测试命令执行

执行：

```text
只运行：

git status
git branch --show-current
node --version
git --version

不要修改仓库。
```

成功以后，这套环境就已经具备实际开发能力。

---

# 二十四、多个项目应该如何运行

假设有：

```text
D:\work\project-a
D:\work\project-b
D:\work\project-c
```

只运行一个 Tunnel：

```powershell
otunnel run --profile chappie
```

然后三个项目各自启动一个 Pi。

终端一：

```powershell
cd D:\work\project-a
pi --provider chappie --model chatgpt
```

终端二：

```powershell
cd D:\work\project-b
pi --provider chappie --model chatgpt
```

终端三：

```powershell
cd D:\work\project-c
pi --provider chappie --model chatgpt
```

此时：

```text
                    ┌── ProjectA
                    │   session A
                    │
ChatGPT → Chappie ──┼── project-b
                    │   session B
                    │
                    └── project-c
                        session C
```

---

# 二十五、多个项目时，ChatGPT 如何知道操作哪一个

不要依赖：

```text
第一个在线项目
```

也不要简单执行：

```text
init({})
```

因为当一个 ChatGPT 对话没有默认绑定时：

```text
init({})
```

会尝试选择第一个：

```text
online
+
unbound
```

的 Pi Session。

多个项目同时在线时，最可靠的方式永远是：

```text
sessions
↓
根据 cwd / name 找项目
↓
取得 sessionId
↓
init(sessionId)
```

例如：

```text
@Chappie

调用 sessions。

找到 cwd 精确等于：

D:\work\project-b

的 session。

用这个 sessionId 调用 init。

如果没有唯一匹配，不要执行任何工具。
```

---

# 二十六、推荐的项目与 ChatGPT 对话关系

最清晰的使用方式是：

```text
ChatGPT 对话
ProjectA 开发
        │
        ▼
Pi Session A
D:\work\project-a
```

```text
ChatGPT 对话
ProjectB 开发
        │
        ▼
Pi Session B
D:\work\project-b
```

```text
ChatGPT 对话
ProjectC 开发
        │
        ▼
Pi Session C
D:\work\project-c
```

也就是：

> 一个长期项目，对应一个主要 ChatGPT 对话和一个 Pi Session。

这样可以避免不同项目的代码、架构和上下文互相污染。

---

# 二十七、绑定完成后是否每次都要指定项目

不需要。

第一次：

```text
sessions
↓
找到 ProjectA
↓
init(sessionId=A)
```

以后当前 ChatGPT 对话会保存：

```text
default session = A
```

后续可以直接说：

```text
检查这个 bug
```

```text
运行测试
```

```text
看看 git diff
```

```text
修改这个组件
```

默认都会进入同一个 Pi Session。

Chappie 把这种情况称为：

```text
existing
```

即复用当前 Chat 已有的默认 session。

---

# 二十八、新建 ChatGPT 对话后怎么办

新的 ChatGPT 对话没有旧对话的默认绑定。

不要直接：

```text
@Chappie init
```

应该：

```text
@Chappie

调用 sessions。

找到：

D:\work\project-a

对应的 session。

然后使用它的 sessionId 调用 init。
```

同一个 Pi Session 可以被多个 ChatGPT 对话使用。

因此可以：

```text
Chat A ─┐
        ├── ProjectA Pi Session
Chat B ─┘
```

---

# 二十九、分支到新聊天后怎么办

如果在 ChatGPT 中使用：

```text
Branch / 分支到新聊天
```

新聊天最好继续使用原来的 Pi `sessionId`。

例如旧任务：

```text
ProjectA
sessionId = abc123
```

新分支继续：

```text
init({
  sessionId: "abc123"
})
```

Chappie 明确支持在新 Chat 或 Branch 中，通过原 session ID 恢复目标 Pi Session。

---

# 三十、一个 ChatGPT 对话能否同时操作多个项目

可以。

例如默认 session：

```text
ProjectA
sessionId = aaa
```

临时读取另一个项目：

```text
project-b
sessionId = bbb
```

可以在具体工具调用里显式传：

```text
sessionId = bbb
```

例如：

```json
{
  "path": "package.json",
  "sessionId": "bbb"
}
```

`read`、`bash`、`edit`、`write` 都可以指定 sessionId。

因此一个 ChatGPT 对话理论上可以：

```text
默认操作 ProjectA

临时读取 ProjectB

再查看 ProjectC
```

但长期开发不建议这样使用。

不同项目最好保持不同 ChatGPT 对话。

---

# 三十一、bindingCount 是什么

`sessions` 中还有一个重要字段：

```text
bindingCount
```

它表示：

```text
有多少个 ChatGPT 对话
保存了这个 Pi Session
作为自己的默认 Session
```

例如：

```text
bindingCount = 0
```

表示还没有对话长期绑定它。

```text
bindingCount = 1
```

表示已有一个 ChatGPT 对话把它作为默认 Session。

```text
bindingCount = 3
```

表示有三个 ChatGPT 对话绑定了这个 Pi Session。

关闭 ChatGPT 对话并不一定立即让这个数字归零，因为保存的绑定仍然可能存在。

---

# 三十二、Pi 下线以后会不会误操作其他项目

正常不会。

假设当前 ChatGPT 对话绑定：

```text
session A
ProjectA
```

然后 ProjectA Pi 被关闭。

此时：

```text
session A = offline
```

Chappie 会继续保留这个绑定。

它不会因为：

```text
project-b 还在线
```

就自动把当前 ChatGPT 对话切换到 project-b。

已有绑定会等待原 Pi Session 重新连接。

这一点对于多个项目非常重要。

---

# 三十三、长时间命令不要直接等待

ChatGPT MCP 工具调用存在超时。

例如：

```text
大型 Rust 编译
Android Gradle 构建
Docker Image Build
大型模型推理
长时间测试
```

不要：

```text
tool call
↓
一直等十分钟
```

而应该让任务后台运行。

Linux / WSL 可以用：

```bash
tmux new-session -d -s build 'npm run build > /tmp/build.log 2>&1'
```

然后立刻返回。

后面再检查：

```bash
tail -100 /tmp/build.log
```

或者：

```bash
tmux capture-pane -pt build
```

整个模式应该是：

```text
调用 1
启动后台任务
↓
立即返回

调用 2
查看状态

调用 3
读取日志

调用 4
读取最终结果
```

而不是让一个 MCP 请求长期占用。

---

# 三十四、图片如何读取

如果项目中有：

```text
screenshot.png
```

可以直接：

```text
读取 screenshot.png，
分析界面问题。
```

Chappie 会把图片内容通过 MCP 发送到 ChatGPT。

并不是：

```text
ChatGPT 云端直接打开你的 C:
```

真实路径是：

```text
本地文件
  ↓
Pi
  ↓
Chappie
  ↓
MCP
  ↓
OpenAI
  ↓
ChatGPT
```

Chappie 的 `read` 可以发送图片，`transfer` 可以进一步把本地文件传到 ChatGPT 云端。

---

# 三十五、文件如何在本地与 ChatGPT 之间传输

Chappie 提供：

```text
transfer
```

可以：

```text
ChatGPT → Pi
```

也可以：

```text
Pi → ChatGPT
```

例如本地生成：

```text
build/output.zip
```

可以通过 transfer 导出。

反过来，如果 ChatGPT 中上传：

```text
reference.png
```

也可以传到：

```text
assets/reference.png
```

完整文件传输由 Chappie 的 transfer 工具完成。

---

# 三十六、Pi 中的 chat 工具有什么作用

ChatGPT 可以调用：

```text
chat
```

把 Markdown 消息显示在 Pi 中。

例如：

```text
已经完成 parser 修改，正在运行测试。
```

会显示在本地 Pi。

反过来，本地 Pi 用户输入的新消息，会随着后续 Chappie Tool Result 一起返回 ChatGPT。

因此它可以形成：

```text
ChatGPT
   ↕
Pi 用户
```

的辅助通信通道。

---

# 三十七、推荐的最终工作方式

日常使用可以固定成下面这一套流程。

## 启动电脑后

第一步：

```powershell
otunnel run --profile chappie
```

---

## 开始 ProjectA

```powershell
cd D:\work\project-a
pi --provider chappie --model chatgpt
```

打开对应 ChatGPT 对话：

```text
@Chappie 继续当前 ProjectA 任务。
```

---

## 开始 ProjectB

另开终端：

```powershell
cd D:\work\project-b
pi --provider chappie --model chatgpt
```

打开另一个 ChatGPT 对话：

```text
@Chappie 继续 ProjectB。
```

---

## 开始 ProjectC

另开终端：

```powershell
cd D:\work\project-c
pi --provider chappie --model chatgpt
```

打开对应 ChatGPT 对话即可。

最终桌面可能只有：

```text
Terminal 1
otunnel

Terminal 2
ProjectA Pi

Terminal 3
project-b Pi

Terminal 4
project-c Pi
```

而 ChatGPT 左侧：

```text
ProjectA 开发
ProjectB 开发
ProjectC Runtime
```

项目和上下文一一对应。

---

# 三十八、推荐的新项目首次连接模板

以后每创建一个新 ChatGPT 对话，可以直接使用：

```text
@Chappie

我要操作项目：

D:\work\project-a

执行以下步骤：

1. 调用 sessions。
2. 根据 cwd 精确找到该项目对应的 Pi Session。
3. 必须确认只有一个匹配。
4. 使用对应 sessionId 调用 init。
5. 输出当前 cwd、Git branch 和 git status。
6. 不要修改任何文件。

如果项目不存在、存在多个匹配、Pi 未在线，
停止操作并告诉我当前 sessions 状态。
```

这可以最大程度避免操作错项目。

---

# 三十九、推荐的正式开发模板

项目绑定成功以后：

```text
@Chappie

处理当前项目中的这个问题。

要求：

1. 先检查 git status。
2. 阅读项目中的 AGENTS.md、README 和相关配置。
3. 定位相关代码。
4. 先确认根因。
5. 使用最小侵入方式修改。
6. 不修改无关代码。
7. 补充必要测试。
8. 运行现有测试。
9. 运行 lint。
10. 运行 typecheck。
11. 运行 build。
12. 最后检查 git diff。
13. 总结修改内容、测试结果和仍存在的风险。

长时间命令不要阻塞 MCP 调用，
使用后台任务执行，再单独查询状态和日志。
```

---

# 四十、故障排查速查

## Tunnel 正常但 MCP Server 不可达

症状：

```text
control_plane_connection PASS
mcp_server_reachable FAIL
```

检查：

```powershell
node -v
npm view @zetaloop/chappie engines
pi --version
pi list
chappie --version
(Get-Content "$HOME\.pi\agent\npm\node_modules\@zetaloop\chappie\package.json" -Raw | ConvertFrom-Json).version
chappie
```

---

## Node 太旧

检查：

```powershell
node -v
```

如果低于 Chappie 当前 `engines` 要求：

```powershell
npm view @zetaloop/chappie engines
```

升级 Node。

---

## 切换 Node 后 pi 消失

重新安装：

```powershell
npm install -g --ignore-scripts @earendil-works/pi-coding-agent
```

然后：

```powershell
pi install npm:@zetaloop/chappie
```

---

## ChatGPT 找不到项目

执行：

```text
sessions
```

根据：

```text
cwd
name
sessionId
status
```

选择项目。

不要直接盲目：

```text
init({})
```

---

## ChatGPT 连接了错误项目

重新：

```text
sessions
```

找到正确 `sessionId`：

```text
init(sessionId)
```

即可切换当前 ChatGPT 对话的默认 Session。

---

## Pi 已启动但 sessions 看不到

确认启动方式：

```powershell
pi --provider chappie --model chatgpt
```

而不是只有：

```powershell
pi
```

---

## Tunnel 启动不了 Pi

检查：

```powershell
where.exe pi
```

确保 otunnel 所在环境能找到 Pi。

---

## API Key 失败

重新检查：

```text
Tunnels
Read
Use
```

以及：

```text
control-plane-api-key-ref
```

指向的文件路径是否正确。

---

# 四十一、安全边界

当 ChatGPT App 配置为：

```text
Allow all operations
```

而 Pi 又拥有：

```text
read
write
edit
bash
```

时，本质上已经形成：

```text
ChatGPT
↓
你的 Windows 用户权限
```

所以至少应遵守：

```text
不要以 Administrator 身份运行 Pi

不要把 Tunnel API Key 提交到 Git

不要把 API Key 写进公开脚本

首次连接先做只读测试

确认 cwd 后再允许修改

多个项目使用 sessionId 精确绑定

重要仓库保持 Git 工作区干净
```

同时需要明确：

> 项目目录并不是系统级沙箱。

如果 Pi 以普通 Windows 用户运行，它原则上拥有这个 Windows 用户能访问的文件权限。

所以：

```text
最好的权限控制
=
操作系统用户权限
+
Git
+
ChatGPT 工具确认
+
明确的项目 Session
```

---

# 四十二、最终架构

完整系统可以归纳成：

```text
                   ChatGPT
                      │
                      │ MCP
                      ▼
              OpenAI Secure Tunnel
                      │
                      ▼
                   otunnel
                      │
                      │ stdio
                      ▼
                chappie
                 MCP Broker
                      │
          ┌───────────┼───────────┐
          │           │           │
          ▼           ▼           ▼
       Session A   Session B   Session C
       ProjectA    ProjectB      ProjectC
          │           │           │
          ▼           ▼           ▼
         Git         Git         Git
        Files       Files       Files
        Build       Build       Build
        Test        Test        Test
```

整个系统中：

```text
ChatGPT
负责：
思考、上下文、推理、规划

OpenAI Tunnel
负责：
云端与本地之间的安全传输

otunnel
负责：
本地 Tunnel Client

chappie
负责：
MCP Broker

pi --provider chappie --model chatgpt
负责：
具体项目 Session

sessionId
负责：
唯一确定当前操作的是哪个项目
```

多个项目的核心管理原则只有一句：

> Tunnel 只运行一个，每个项目运行一个 Pi Session，每个长期项目对应一个 ChatGPT 对话，新对话先通过 `sessions → cwd → sessionId → init` 精确绑定。

这样，ChatGPT 网页版就不再只是一个能阅读粘贴代码的聊天窗口，而变成了真正能够进入本地项目、理解代码、修改代码、运行工具、验证结果，并且同时管理多个开发环境的远程工程工作台。
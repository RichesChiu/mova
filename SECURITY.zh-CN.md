# 安全策略

[English](SECURITY.md) · 简体中文

## 报告安全漏洞

不要在公开 Issue、Pull Request、讨论或聊天中披露疑似安全漏洞。

优先使用 [GitHub 私密漏洞报告](https://github.com/RichesChiu/mova/security/advisories/new)。如果该功能不可用，请发送邮件到 `riches.chiu@gmail.com`，并提供：

- 受影响的 Mova 版本或镜像 digest；
- 受影响的接口、解析器、文件类型或部署路径；
- 可复现步骤或最小化概念验证；
- 预期安全影响；
- 已知临时解决办法。

不要附带真实凭据、私有媒体或个人数据。维护者会确认报告、验证影响范围，并在修复或缓解措施可用后协调披露。

## 支持的版本

安全修复面向当前稳定版本。Preview 是评估渠道，通过后续 Preview 或稳定版本获得修复。生产部署应使用不可变版本标签，并升级到最新稳定补丁版本。

## 容器发布策略

官方镜像支持 Linux `amd64` 和 `arm64`。不可变版本提升前，会对同一个候选 manifest 在两个平台执行烟测和安全扫描。发布门禁会：

- 无缓存刷新 Debian 运行时基础镜像；
- 报告所有 Critical 和 High 发现；
- 阻止存在可修复的 Critical 或 High 漏洞；
- 阻止 CISA Known Exploited Vulnerabilities 目录中的漏洞；
- 要求逐项审查暂无上游修复的残留发现。

只有仓库与二进制证据证明漏洞路径不存在或不可达时，才能使用 VEX `not_affected`。可能触达但尚未修复的漏洞必须保持可见，并按精确 CVE 在当次发布中接受；不允许宽泛或静默豁免。

容器扫描只能降低风险，不能保证任意媒体文件可信。请只挂载可信媒体，及时更新 Mova 与宿主机运行环境，并在公开服务前配置 HTTPS 和带身份验证的反向代理。

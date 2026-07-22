import { useI18n } from '../i18n-context'
import { githubUrl } from '../data/homeContent'
import './LegalPage.css'

const supportEmail = 'riches.chiu@gmail.com'
const issueUrl = `${githubUrl}/issues/new`

export function SupportPage({ onOpenPrivacy }: { onOpenPrivacy: () => void }) {
  const { language } = useI18n()
  const isChinese = language === 'zh'

  return (
    <article className="legal-page support-page">
      <div className="legal-page-inner">
        <header className="legal-page-header">
          <p className="legal-page-kicker">MOVA · SUPPORT</p>
          <h1>{isChinese ? 'MOVA 支持' : 'MOVA Support'}</h1>
          <p className="legal-page-summary">
            {isChinese
              ? '获取 macOS 客户端连接、登录、媒体库与播放问题的帮助。'
              : 'Get help with server connections, sign-in, libraries, and playback in the MOVA macOS app.'}
          </p>
        </header>

        <section className="support-contact-card">
          <div>
            <h2>{isChinese ? '联系开发者' : 'Contact the developer'}</h2>
            <p>
              {isChinese
                ? 'MOVA 是开源项目。可复现的问题、兼容性反馈和功能建议请优先提交 GitHub Issue；包含隐私信息时请使用邮件。'
                : 'MOVA is open source. Use GitHub Issues for reproducible bugs, compatibility feedback, and feature requests; use email for anything private.'}
            </p>
          </div>
          <div className="support-contact-actions">
            <a className="support-issue-link" href={issueUrl} target="_blank" rel="noreferrer">
              {isChinese ? '提交 GitHub Issue' : 'Open a GitHub Issue'}
            </a>
            <a className="support-email-link" href={`mailto:${supportEmail}`}>{supportEmail}</a>
          </div>
        </section>

        {isChinese ? (
          <ChineseSupport onOpenPrivacy={onOpenPrivacy} />
        ) : (
          <EnglishSupport onOpenPrivacy={onOpenPrivacy} />
        )}
      </div>
    </article>
  )
}

function ChineseSupport({ onOpenPrivacy }: { onOpenPrivacy: () => void }) {
  return (
    <div className="legal-sections">
      <section className="legal-section">
        <h2>开始使用</h2>
        <ol>
          <li>准备一个可访问且版本兼容的 MOVA 自托管服务器。</li>
          <li>在服务器列表中添加服务器地址；公网地址建议使用 HTTPS。</li>
          <li>使用该服务器中的账户登录，然后浏览媒体库或由管理员创建和扫描媒体库。</li>
        </ol>
        <p>MOVA App 不附带媒体内容，也不提供公共流媒体服务。</p>
      </section>

      <section className="legal-section">
        <h2>连接或登录失败</h2>
        <ul>
          <li>确认服务器地址包含正确协议、域名或 IP 及端口。</li>
          <li>在浏览器中检查服务器的 <code>/api/health</code> 是否可访问。</li>
          <li>本地服务器请允许 MOVA 使用本地网络；公网服务器请检查 HTTPS 证书和反向代理配置。</li>
          <li>确认账号属于当前服务器，并且服务器与客户端使用兼容的 API / SSE 契约。</li>
        </ul>
      </section>

      <section className="legal-section">
        <h2>媒体库或扫描问题</h2>
        <ul>
          <li>创建、编辑、删除及扫描媒体库需要管理员权限。</li>
          <li>媒体目录来自服务端容器内的可选路径，不是当前 Mac 的本地路径。</li>
          <li>扫描过程通过服务器实时事件显示；如果长期无进展，请一并提供服务端日志和扫描任务信息。</li>
        </ul>
      </section>

      <section className="legal-section">
        <h2>播放问题</h2>
        <ul>
          <li>确认媒体文件仍存在且 MOVA 服务端进程有读取权限。</li>
          <li>拖动后卡顿、画面异常或字幕问题请注明资源容器、视频编码、音轨和字幕格式。</li>
          <li>请说明问题是否只发生在某一资源，并附上发生时间点；不要发送完整私人媒体文件。</li>
        </ul>
      </section>

      <section className="legal-section">
        <h2>发送有效的诊断信息</h2>
        <p>邮件中建议包含：</p>
        <ul>
          <li>MOVA App 版本和构建号、macOS 版本、Mac 芯片型号。</li>
          <li>MOVA 服务端版本、部署方式以及可复现步骤。</li>
          <li>经过脱敏的错误文案、截图或相关日志片段。</li>
        </ul>
        <p className="support-note">请勿发送账户密码、访问令牌、私钥或含私人信息的完整媒体文件。</p>
      </section>

      <section className="legal-section">
        <h2>隐私与数据请求</h2>
        <p>
          App 中的媒体和账户数据由你选择的服务器处理。详情请查看
          <button className="legal-page-link" type="button" onClick={onOpenPrivacy}>
            MOVA 隐私政策
          </button>。
        </p>
      </section>
    </div>
  )
}

function EnglishSupport({ onOpenPrivacy }: { onOpenPrivacy: () => void }) {
  return (
    <div className="legal-sections">
      <section className="legal-section">
        <h2>Getting started</h2>
        <ol>
          <li>Prepare an accessible, compatible MOVA self-hosted server.</li>
          <li>Add its address in the server list. Use HTTPS for public servers.</li>
          <li>Sign in with an account on that server, then browse or let an administrator create and scan libraries.</li>
        </ol>
        <p>The MOVA app includes no media and does not provide a public streaming service.</p>
      </section>

      <section className="legal-section">
        <h2>Connection or sign-in failures</h2>
        <ul>
          <li>Confirm that the address includes the correct scheme, host or IP, and port.</li>
          <li>Check whether the server's <code>/api/health</code> endpoint is reachable in a browser.</li>
          <li>Allow Local Network access for a local server. For a public server, check its TLS certificate and reverse proxy.</li>
          <li>Confirm that the account belongs to this server and that client and server use compatible API and SSE contracts.</li>
        </ul>
      </section>

      <section className="legal-section">
        <h2>Library or scan issues</h2>
        <ul>
          <li>Creating, editing, deleting, and scanning libraries requires administrator permission.</li>
          <li>Library paths come from the server container and are not local paths on the current Mac.</li>
          <li>Scan progress arrives through realtime server events. If it stalls, include server logs and scan job details.</li>
        </ul>
      </section>

      <section className="legal-section">
        <h2>Playback issues</h2>
        <ul>
          <li>Confirm that the media file exists and the MOVA server can read it.</li>
          <li>For seek stalls, video artifacts, or subtitle issues, include the container, video codec, audio track, and subtitle format.</li>
          <li>State whether the issue affects one file and include the playback timestamp. Do not send an entire private media file.</li>
        </ul>
      </section>

      <section className="legal-section">
        <h2>Send useful diagnostics</h2>
        <p>Please include:</p>
        <ul>
          <li>MOVA app version and build, macOS version, and Mac chip.</li>
          <li>MOVA server version, deployment method, and reproduction steps.</li>
          <li>Redacted error text, screenshots, or relevant log excerpts.</li>
        </ul>
        <p className="support-note">Never send passwords, access tokens, private keys, or complete media files containing private information.</p>
      </section>

      <section className="legal-section">
        <h2>Privacy and data requests</h2>
        <p>
          Media and account data in the app is handled by the server you select. See the{' '}
          <button className="legal-page-link" type="button" onClick={onOpenPrivacy}>
            MOVA Privacy Policy
          </button>{' '}
          for details.
        </p>
      </section>
    </div>
  )
}

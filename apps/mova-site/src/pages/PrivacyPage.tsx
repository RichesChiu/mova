import { useI18n } from '../i18n-context'
import './LegalPage.css'

const supportEmail = 'riches.chiu@gmail.com'

export function PrivacyPage() {
  const { language } = useI18n()
  const isChinese = language === 'zh'

  return (
    <article className="legal-page">
      <div className="legal-page-inner">
        <header className="legal-page-header">
          <p className="legal-page-kicker">MOVA · PRIVACY</p>
          <h1>{isChinese ? '隐私政策' : 'Privacy Policy'}</h1>
          <p className="legal-page-summary">
            {isChinese
              ? 'MOVA 是连接用户所选自托管服务器的原生媒体客户端。我们不经营集中式媒体云，也不通过 App 收集你的媒体库、播放记录或账号凭据。'
              : 'MOVA is a native media client that connects to a self-hosted server selected by you. We do not operate a centralized media cloud or collect your library, playback history, or account credentials through the app.'}
          </p>
          <div className="legal-page-meta">
            <span>{isChinese ? '生效日期：2026 年 7 月 20 日' : 'Effective: July 20, 2026'}</span>
          </div>
        </header>

        {isChinese ? <ChinesePrivacy /> : <EnglishPrivacy />}
      </div>
    </article>
  )
}

function ChinesePrivacy() {
  return (
    <div className="legal-sections">
      <section className="legal-section">
        <h2>1. 适用范围</h2>
        <p>
          本政策适用于 MOVA macOS 客户端及 mova.hk 官网。你自行部署或由第三方运营的 MOVA
          服务器是独立的数据处理环境，其运营者应对服务器上的账户、媒体与日志承担相应责任。
        </p>
      </section>

      <section className="legal-section">
        <h2>2. App 如何处理数据</h2>
        <h3>保存在你的 Mac 上</h3>
        <ul>
          <li>服务器名称、服务器地址、登录账号名、界面语言、主题及其他使用偏好保存在本机。</li>
          <li>访问令牌和刷新令牌保存在 macOS 钥匙串中。</li>
          <li>登录密码仅用于向你选择的 MOVA 服务器发起登录请求，App 不会持久保存明文密码。</li>
        </ul>
        <h3>发送到你选择的服务器</h3>
        <p>
          登录、媒体浏览、搜索、播放、播放进度、通知状态和服务器管理操作会直接发送到你配置的 MOVA
          服务器。服务器会向 App 返回账户资料、媒体元数据、图片、资源信息及播放内容。MOVA 开发者和
          mova.hk 不会接收这些请求的副本。
        </p>
        <p>
          如果服务器由他人提供，请在登录前了解该服务器运营者的隐私规则。连接公网服务器时建议使用有效的
          HTTPS 配置，避免敏感信息通过不安全网络传输。
        </p>
      </section>

      <section className="legal-section">
        <h2>3. 我们不收集的内容</h2>
        <p>MOVA 当前不包含广告、跨 App 跟踪、第三方分析或由开发者运营的崩溃上报服务。</p>
        <ul>
          <li>我们不会出售或出租个人信息。</li>
          <li>我们不会将媒体库、观看历史或搜索记录发送至开发者服务器。</li>
          <li>仅在设备上处理且未发送给开发者的数据，不构成由我们收集的数据。</li>
        </ul>
      </section>

      <section className="legal-section">
        <h2>4. 官网与支持邮件</h2>
        <p>
          mova.hk 不设置广告或分析跟踪 Cookie。网站当前通过 GitHub Pages 托管；GitHub
          可能为安全、运行和访问日志目的处理 IP 地址、浏览器信息和请求记录，相关处理遵循 GitHub
          自己的隐私政策。
        </p>
        <p>
          当你发送支持邮件时，我们会收到你的邮箱地址、邮件内容以及你主动提供的附件。信息仅用于回答问题、排查故障和维护必要的沟通记录。请勿发送密码、访问令牌或不必要的私人媒体内容。
        </p>
      </section>

      <section className="legal-section">
        <h2>5. 数据控制与删除</h2>
        <ul>
          <li>你可以在服务器列表中删除配置，并移除对应的本机令牌。</li>
          <li>删除 App 会移除 App 容器内的本地设置；钥匙串项目也可由你通过系统钥匙串工具管理。</li>
          <li>服务器上的账户、播放记录和媒体数据应向该服务器的运营者申请查询、更正或删除。</li>
          <li>支持邮件相关信息可通过下方邮箱请求查阅或删除，但依法或为解决争议必须保留的内容除外。</li>
        </ul>
      </section>

      <section className="legal-section">
        <h2>6. 儿童隐私</h2>
        <p>
          MOVA 不是专门面向儿童设计的服务，我们不会明知而向儿童收集个人信息。由家长或监护人部署的家庭媒体服务器，应由其负责账户与内容访问管理。
        </p>
      </section>

      <section className="legal-section">
        <h2>7. 政策变更</h2>
        <p>
          如果 App 的数据处理方式发生实质变化，我们会更新本页面和生效日期，并同步更新 App Store
          Connect 中的隐私披露。重大变更会通过合理方式另行提示。
        </p>
      </section>

      <section className="legal-section">
        <h2>8. 联系方式</h2>
        <p>
          隐私问题、数据请求或投诉请联系 MOVA 支持：{' '}
          <a href={`mailto:${supportEmail}`}>{supportEmail}</a>
        </p>
      </section>
    </div>
  )
}

function EnglishPrivacy() {
  return (
    <div className="legal-sections">
      <section className="legal-section">
        <h2>1. Scope</h2>
        <p>
          This policy applies to the MOVA macOS app and mova.hk. A MOVA server that you self-host or
          access through a third party is a separate data-processing environment. Its operator is
          responsible for the accounts, media, and logs held on that server.
        </p>
      </section>

      <section className="legal-section">
        <h2>2. How the app handles data</h2>
        <h3>Stored on your Mac</h3>
        <ul>
          <li>Server names and addresses, account names, language, theme, and preferences are stored locally.</li>
          <li>Access and refresh tokens are stored in the macOS Keychain.</li>
          <li>Your password is used to sign in to the MOVA server you selected. The app does not persist the plaintext password.</li>
        </ul>
        <h3>Sent to your selected server</h3>
        <p>
          Sign-in, browsing, search, playback, progress, notification state, and server administration
          requests go directly to the MOVA server you configure. That server returns account details,
          media metadata, artwork, resource information, and media streams. The MOVA developer and mova.hk do
          not receive a copy of those requests.
        </p>
        <p>
          If someone else operates the server, review their privacy practices before signing in. Use a
          valid HTTPS configuration for public servers to reduce the risk of exposing sensitive data in transit.
        </p>
      </section>

      <section className="legal-section">
        <h2>3. Data we do not collect</h2>
        <p>
          MOVA currently contains no advertising, cross-app tracking, third-party analytics, or
          developer-operated crash reporting.
        </p>
        <ul>
          <li>We do not sell or rent personal information.</li>
          <li>We do not send your library, watch history, or searches to a developer-operated server.</li>
          <li>Information processed only on your device and not sent to us is not collected by us.</li>
        </ul>
      </section>

      <section className="legal-section">
        <h2>4. Website and support email</h2>
        <p>
          mova.hk does not set advertising or analytics cookies. The site is currently hosted on GitHub
          Pages. GitHub may process IP addresses, browser information, and request logs for security and
          operational purposes under its own privacy policy.
        </p>
        <p>
          If you email support, we receive your email address, message, and attachments you choose to
          send. We use them only to respond, troubleshoot, and retain necessary support records. Do not
          send passwords, access tokens, or unnecessary private media.
        </p>
      </section>

      <section className="legal-section">
        <h2>5. Your controls and deletion</h2>
        <ul>
          <li>You can remove a saved server configuration and its local tokens from the server list.</li>
          <li>Deleting the app removes settings in its app container. You can also manage Keychain items with macOS tools.</li>
          <li>Contact the relevant server operator to access, correct, or delete accounts, progress, or media data held there.</li>
          <li>You may request access to or deletion of support correspondence, except where retention is legally required or necessary to resolve a dispute.</li>
        </ul>
      </section>

      <section className="legal-section">
        <h2>6. Children</h2>
        <p>
          MOVA is not specifically directed to children, and we do not knowingly collect personal
          information from children. A parent or guardian operating a household server is responsible for
          account and content access controls.
        </p>
      </section>

      <section className="legal-section">
        <h2>7. Changes to this policy</h2>
        <p>
          If the app's data practices materially change, we will update this page, its effective date,
          and the disclosures in App Store Connect. We will provide an additional reasonable notice for
          significant changes.
        </p>
      </section>

      <section className="legal-section">
        <h2>8. Contact</h2>
        <p>
          For privacy questions, data requests, or complaints, contact MOVA support at{' '}
          <a href={`mailto:${supportEmail}`}>{supportEmail}</a>.
        </p>
      </section>
    </div>
  )
}

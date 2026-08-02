# NFO 本地元数据契约

本文定义 MOVA 对 Kodi / Emby 兼容 NFO 的读取范围、字段作用域、来源选择、TMDB 补全、持久化与安全边界。NFO 是用户拥有的本地元数据来源；MOVA 只读取，不创建、格式化或写回媒体目录中的 NFO。

MOVA 采用 Kodi 的 NFO XML 结构作为交换格式，并借鉴 Emby 的“保留已有本地字段、远端补充缺失字段”行为，但不承诺复制 Emby 的数据库、锁定编辑器或刷新模式。

- [Kodi NFO files](https://kodi.wiki/view/NFO_files)
- [Kodi Movie NFO](https://kodi.wiki/view/NFO_files/Movies)
- [Kodi TV Show NFO](https://kodi.wiki/view/NFO_files/TV_shows)
- [Kodi Episode NFO](https://kodi.wiki/view/NFO_files/Episodes)
- [Emby Metadata Manager](https://support.emby.media/support/articles/Metadata-manager.html)

## 1. 元数据来源与优先级

不同类别的数据使用不同权威来源，不能用一个全局优先级覆盖所有字段。

### 1.1 作品身份

作品身份指媒体类型以及已经接受的 provider binding。

1. 管理员明确执行“搜索 / 替换元数据”后接受的身份最高，不会被后续 NFO 或自动扫描静默换绑。
2. 已经验证并持久化的 TMDB binding 继续作为当前作品身份。
3. 未绑定条目可以把类型正确、值唯一的 NFO TMDB ID 作为 direct lookup 提示；只有 TMDB 返回对应类型的详情后才写成 binding。
4. NFO、容器显式 ID 或同容器既有 binding 给出不同身份时，记录扫描冲突，不自动采用其中一个。
5. 没有可用 direct lookup 提示时，才按标题和年份规则搜索 TMDB。

ID 的作用域必须严格隔离：

- `movie` 根中的 TMDB ID 只提示 movie lookup。
- `tvshow` 根中的 TMDB ID 只提示 series lookup。
- `episodedetails` 根中的 TMDB ID 只属于该 episode 的外部身份，不参与 series lookup，也不能绑定或改写父剧。
- 类型不一致、格式无效或与已有 accepted binding 冲突的 ID 只保留在来源快照中，不参与自动绑定。

存在 NFO 不等于已经完成远端匹配，也不等于跳过 TMDB。

### 1.2 展示字段

标题、简介、年份、题材、国家、制作方和图片等展示字段按字段分别合并：

```text
选中 NFO 中实际存在的非空字段
> 当前已经写入条目的 TMDB 展示字段
> 文件名、受限目录结构和本地推断
```

NFO 没有声明的字段可以由 TMDB 补齐。普通扫描和元数据刷新不会用 TMDB 覆盖选中 NFO 实际提供的同名字段。“搜索 / 替换元数据”用于明确改变 provider binding 并立即应用该远端条目，但当前版本没有逐字段人工编辑器，也不会由该操作创建永久的人工字段锁；后续扫描仍按 NFO 字段所有权重新生成展示投影。

### 1.3 多值与命名空间数据

- `genre`、`country`、`studio`、`tag` 按 NFO 顺序去重后保存；兼容 API 所需的展示字符串由结构化值生成。
- external ID 和评分按 `provider/source + retrieved_via` 共存。TMDB 刷新只替换 TMDB 写入的记录，不删除 NFO 来源记录。
- NFO 演员、导演和编剧作为本地演职员集合持久化，不用 TMDB cast cache 代替，也不在数据库层截断。

### 1.4 技术信息与用户状态

- 文件容器、时长、编码、分辨率、音轨和字幕以实际文件与 `ffprobe` 为权威。NFO `runtime` 和 `fileinfo/streamdetails` 不能覆盖探测结果。
- 播放进度、已看状态、继续观看、收藏和最近播放按 MOVA 用户保存，不从共享 NFO 导入。

## 2. XML 文档要求

MOVA 只读取完整、良构的 UTF-8 XML 文档。“完整 XML”表示文档必须有且只有一个允许的根元素，不表示每个元数据字段都必须填写；允许在合法根元素内只声明部分字段。

允许的根元素：

| 用途 | 根元素 |
| --- | --- |
| 电影 | `movie` |
| 电视剧父条目 | `tvshow` |
| 单集 | `episodedetails` |

不接受 Kodi 的 URL-only NFO、XML 后追加 URL 的 combination NFO、多个顶层根元素、HTML、DTD 或外部实体。未知 XML 节点被忽略且不会原样持久化。

根元素必须与文件用途一致。电影不能读取 `tvshow`，单集不能把 `movie` 或 `tvshow` 当作单集元数据。根元素不一致等同于该候选无效，并记录可诊断信息。

## 3. 文件发现和 MOVA 选择规则

### 3.1 电影

电影按以下顺序寻找候选：

1. 与视频同名的 `<stem>.nfo`。
2. 同目录的 `movie.nfo`。

候选必须以 `movie` 为根。`<stem>.nfo` 有效时不再回退 `movie.nfo`；同名文件存在但无效时使用 last-known-good 规则，不用通用文件静默掩盖错误。

`movie.nfo` 是目录级候选，不因与视频处于同一目录就自动归属全部视频。扫描计划先使用不含 NFO 的文件名结果计算该目录中全部电影载体的身份；只有这些载体的规范化标题与年份形成唯一逻辑作品时，`movie.nfo` 才能参与解析和投影。目录只有一个逻辑电影组时可以使用；同名同年的 1080p、2160p 等多版本仍属于同一组。目录同时包含不同标题、不同年份或其它无法证明同属一部作品的电影时，通用候选不归属其中任何条目，各视频仍可使用自己的 `<stem>.nfo`。

### 3.2 电视剧父条目

从视频所在目录开始，向上最多检查五层目录中的最近 `tvshow.nfo`，并且不得越过媒体库根目录。该文件只负责 series 字段和 series 级 TMDB ID，不向单集写入标题、简介或 episode ID。

每个 `tvshow.nfo` 的归属在读取其字段前由完整浅层文件清单证明：其目录边界内、最多五层可到达该来源的全部单集必须在不使用 NFO 时已经归入同一个 series group。一个剧集的不同季、不同集和同集多版本可以共享同一来源。包含多部剧集的公共媒体库根目录或合集目录中的 `tvshow.nfo` 不能成为共享候选，即使其中一部分剧集有更近的独立 NFO；媒体库根本身只有一部可证明的剧集时，根目录 `tvshow.nfo` 才可以使用。

同一 series group 内发现多份 `tvshow.nfo` 时保留全部有效快照，只选择一份生成父条目投影，不逐字段拼接不同文档。

### 3.3 单集

单集只读取与视频同名、根为 `episodedetails` 的 `<stem>.nfo`。它不回退到 `movie.nfo`，也不把附近的 `tvshow.nfo` 当作单集文档。

单集 NFO 的 `title`、`plot`、`aired`、评分、演职员、图片和外部 ID 只写 episode。`season` 与 `episode` 只校验文件名已经确定的季集坐标；不一致时记录冲突，不能静默把物理文件移动到另一个季或另一集。

### 3.4 多版本与多来源选择

同一电影的多个版本，或同一季集坐标的多个物理版本，可以带各自 NFO。所有有效来源都保存，公共条目只选择一份稳定投影：

1. 有已接受 binding 时，优先选择 provider ID 与 binding 一致的来源；冲突来源只保留快照和诊断，不进入公共投影。
2. 在身份兼容且字段作用域相同的来源中，视频同名 NFO 优先于通用 `movie.nfo`；同级 `tvshow.nfo` 来源按下一条规则选择。
3. 同级候选仍有多个时，按规范化 `source_path` 字典序选择，不能依赖文件系统遍历顺序。

没有已接受 binding 且候选声明多个不同 TMDB ID 时，不进行 direct lookup；服务端记录身份冲突。非 ID 展示字段仍从按上述稳定规则选中的单份文档投影，不跨来源混合。

## 4. 电影、父剧与单集字段作用域

“投影”表示进入常用媒体字段或结构化关系表；“快照”表示进入版本化标准 NFO payload，供来源查看和以后扩展。原始 XML、未知标签以及未列出的私有扩展不会保存。除特别说明外，下表字段在 `movie`、`tvshow` 和 `episodedetails` 三种合法根中都可解析，但始终只作用于各自层级。

| XML 字段及别名 | 标准化结果 | 当前投影行为 |
| --- | --- | --- |
| `title`、`localtitle`、`name` | `title`，按左侧顺序取首个非空值 | 投影为电影、系列或单集标题 |
| `originaltitle` | `original_title` | 投影到当前层级 |
| `sorttitle`、`sortname` | `sort_title`，按顺序取首个非空值 | 投影到当前层级 |
| `plot`、`outline` | 分别保存 `overview`、`outline` | `plot` 优先投影简介；缺少 `plot` 时才使用 `outline` |
| `tagline` | `tagline` | 投影到当前层级 |
| `year` | 四位年份 | 电影和系列投影年份；单集保留年份并参与单集字段投影 |
| `premiered`、`releasedate` | `premiered`，按顺序取首个非空值 | 投影首映/首播日期 |
| `aired` | `aired` | 单集优先投影为首播日期；其它根保留快照 |
| `mpaa`、`contentrating`、`certification` | `content_rating`，按顺序取首个非空值 | 投影内容分级 |
| `customrating` | `custom_rating` | 仅保留 Emby 自定义分级快照，不能冒充正式内容分级 |
| `status`、`runtime` | 状态文本、正数分钟 | 保留快照；`runtime` 不覆盖 ffprobe 时长 |
| `originallanguage`、`original_language` | `original_language` | 保留作品原始语言快照 |
| `language` | `preferred_metadata_language` | 保留 NFO 元数据文本语言，不与作品原始语言混用 |
| `countrycode` | `preferred_metadata_country_code` | 保留快照，不用于自动猜测作品国家 |
| episodedetails 根的 `showtitle` | `show_title` | 仅保留单集来源信息，不用于反向绑定父剧 |
| `biography`、`review` | `overview` 兼容别名 | 只在缺少 `plot` 时补充简介 |
| `formed` | `premiered` 兼容别名 | 保留来源日期；`premiered` / `releasedate` 优先 |
| `dateadded` | `date_added` | 仅保留快照，不覆盖 MOVA `created_at`，不改变“最新添加”排序 |
| `enddate`、`displayorder`、`aspectratio`、`top250` | 对应标准字段 | 保留快照；不会覆盖播放或 ffprobe 技术事实 |
| 重复 `trailer` | `trailers[]` | 全部去重保存为来源快照；当前不会主动请求或播放其中 URL |
| tvshow 根的 `airs_dayofweek`、`airs_time` | `air_days[]`、`air_time` | 保留 Emby 播出计划快照，当前不参与客户端排序 |
| movie 根的 `showlink` | `show_link` | 保留 Kodi 电影到剧集名称关联，当前不建立数据库关系 |
| 重复 `genre` | `genres[]` | 每个节点还按 `/` 拆分，去空白、大小写不敏感去重；电影/系列生成展示题材 |
| 重复 `country` | `countries[]` | 每个节点还按 `/` 拆分并去重；电影/系列生成展示国家/地区 |
| 重复 `studio` | `studios[]` | 按节点去重；电影/系列生成展示制作方，不按 `/` 拆分 |
| 重复 `tag`、`style` | `tags[]`、`styles[]` | 去重后保留快照，不作为访问控制或自动分类规则 |
| tvshow 根的 `namedseason number="N"` | 季元数据中的 `title` | 仅合法非负季号；同季后值覆盖前值，按季号排序，并投影到已存在季的标题 |
| tvshow 根的 `seasonplot number="N"` | 同一季元数据中的 `overview` | 投影到已存在季的简介 |
| tvshow 根的 `season`、`episode` | `season_count`、`episode_count` | 保存系列声明的季数和集数快照，不误当成单集坐标 |
| episodedetails 根的 `season`、`episode` | 单集坐标提示 | 只校验文件名已确定的坐标，不移动或重建物理结构 |
| `displayepisode` / `airsbefore_episode`、`displayseason` / `airsbefore_season`、`displayafterseason` / `airsafter_season` | 三个特殊排序数字 | 仅保留快照；当前普通季集排序模型不投影特别篇插入关系 |

身份、评分、演职员和图片采用结构化子对象：

| XML 结构 | 标准化与持久化行为 |
| --- | --- |
| `uniqueid type="…"` / `provider="…"` | 保存 provider、值和 `default`；`themoviedb` 归一化为 `tmdb`、`thetvdb` 归一化为 `tvdb`，同 provider + value 去重 |
| `tmdbid` / `tmdb_id`、`imdbid` / `imdb_id`、`tvdbid` / `tvdb_id` | 兼容旧字段并归一化为 external ID；严格遵守 movie、series、episode 作用域 |
| 旧式 `id` 的 `TMDB` / `TVDB` / `IMDB` 属性 | 兼容 Emby 系 NFO 并归一化为 external ID；元素正文只有严格匹配 `tt` + 数字时才按 IMDb 读取，模糊纯数字不猜 provider |
| tvshow `episodeguide` JSON | 保存 provider → ID 的独立快照；不执行 URL，当前不作为自动 binding 提示 |
| `ratings/rating` | 按 `name` 保存来源、`max`、`value`、`votes` 和 `default`；未知来源默认归为 `audience` |
| 根级 `rating` + `votes` | 保存为 `source=default`、10 分制 `audience` 评分 |
| `communityrating` | 支持点号或逗号小数，保存为 `source=community`、10 分制 `audience` 评分 |
| `ratings/rating` 中的番茄影评人、`metacritic` 或明确 critic 来源 | 保存为结构化 `critic` 评分；番茄 audience 和未知来源仍为 `audience` |
| `criticrating` | 保存为 `source=default`、100 分制 `critic` 评分；不会误记为 audience |
| `actor` | 保存所有演员的 `name`、`role`、`order`、`thumb`、`type`、`profile` / `biography`，以及 actor 内的 `uniqueid` 和旧式 provider ID；不截断人数 |
| actor 的 `clear="true"` | 保存 `clear_actors` 兼容标记；在服务端定义“空字段拥有所有权”前不清空已有演职员 |
| actor 的 TMDB ID 与头像 | actor TMDB ID 可投影为演职员 `person_id`；通过本地边界校验或可信 TMDB 地址校验的 `thumb` 可持久化为 `profile_path`，但公开 API 会隐藏本机绝对路径；profile 文本仍在标准 payload 中 |
| `director` | 保存所有非空导演节点，不按 `/` 拆分 |
| `credits`、`writer` | 保存所有编剧；每个节点按 `/` 拆分、去空白并去重 |
| `thumb`、`fanart`、`art`、`logo`、`clearlogo` | 保存类型不丢失的图片清单，并同步生成 poster、backdrop、logo、thumbnail 便捷投影；banner、landscape、clearart、discart、keyart 保持原类型 |
| tvshow 的 `thumb type="season" season="N"` | 进入对应季的结构化图片，绝不冒充系列封面；poster/backdrop 可投影到已存在季 |
| `set` | 保存电影合集名称、简介和 external IDs；兼容 `tmdbcolid` 属性并归一化为 `tmdb_collection`，当前不投影为独立合集条目 |
| `lockdata`、`lockedfield`、`lockedfields` | 解析并保存在标准 payload；只作为来源兼容信息，不建立服务端字段锁 |

评分标准结构：

```xml
<ratings>
  <rating name="themoviedb" max="10" default="true">
    <value>8.4</value>
    <votes>18234</votes>
  </rating>
</ratings>
```

外部 ID 标准结构：

```xml
<uniqueid type="tmdb" default="true">12345</uniqueid>
<uniqueid type="imdb">tt1234567</uniqueid>
<uniqueid type="tvdb">67890</uniqueid>
```

图片与合集结构示例：

```xml
<thumb aspect="poster">poster.jpg</thumb>
<thumb aspect="clearlogo">logo.png</thumb>
<fanart>
  <thumb>fanart-1.jpg</thumb>
</fanart>
<thumb aspect="poster" type="season" season="2">season02-poster.jpg</thumb>
<seasonplot number="2">第二季简介</seasonplot>
<set>
  <name>合集名称</name>
  <overview>合集简介</overview>
</set>
```

## 5. 多集文件兼容边界

Kodi v21 及更早版本允许在一个 NFO 中顺序堆叠多个 `episodedetails`，该内容不是单根良构 XML。Kodi v22 改为一个多集视频对应多份带 `-SxxEyy` 后缀的独立 episode NFO 和图片。

MOVA 的业务模型是一条物理媒体文件对应一个 episode 坐标，因此不支持以下两种输入：

- Kodi v21 及更早版本的多顶层 `episodedetails` 堆叠文件。
- Kodi v22 的同一物理视频拆分为多份 `-SxxEyy.nfo` 的多集映射。

这类文件不会被猜测为第一集，也不会把多份单集元数据合并。需要先将视频拆成单集文件，或为 MOVA 增加“一文件多逻辑集”的独立数据模型、播放范围和进度语义后再启用支持。

## 6. Emby 锁语法的记录边界

MOVA 识别并标准化保存：

```xml
<lockdata>true</lockdata>
<lockedfields>Name|Overview|Genres</lockedfields>
```

`lockedfields` 接受 Emby 常见的 `|`、`,` 分隔形式；空值被忽略。当前兼容范围仅包括解析、标准化 payload、`is_locked` 展示和来源追踪。`lockdata` 与 `lockedfields` 不参与字段合并决策，不会阻止 TMDB 补齐缺失字段，也不会锁住 ffprobe 技术信息、文件结构、用户状态或已删除媒体。

MOVA 当前不提供与 Emby Metadata Manager 等价的逐字段编辑器或字段锁，也不把锁值写回 NFO。NFO 非空字段优先于普通 TMDB 补全来自 NFO 来源规则本身，与锁标记无关。

## 7. 持久化与 last-known-good

常用字段投影到 `media_items`、季集结构和通用关系表。每份成功解析的 NFO 保存为 `media_local_metadata_sources` 中的标准化 payload，记录来源路径、文档类型、schema 版本、锁状态和是否为选中来源。只保存受支持字段，不保存原始 XML。

刷新遵守 last-known-good 语义：

- NFO 成功解析并通过根类型校验时，以新标准化快照替换同一路径的旧快照。
- NFO 仍存在，但读取失败、超限、XML 损坏、编码错误或根类型不符时，保留同一路径最近一次成功快照和公共投影，同时记录可诊断问题；不能用一次坏写入擦除已知有效元数据。
- 新条目第一次遇到无效 NFO 时没有 last-known-good，可继续使用文件名、本地图片、ffprobe 和 TMDB 入库。
- 只有一次权威文件树发现确认 NFO 已删除时，才删除对应快照，并从其它选中 NFO、当前已写入条目的远端字段和本地推断重新生成投影。
- NFO 删除与 NFO 临时不可读必须区分；取消扫描、部分发现或媒体根目录不可用不能触发来源清理。

自动 TMDB 刷新只更新自身来源记录。TMDB 内部快照用于判断远端字段所有权、复核和保留期限，不是对客户端开放的离线元数据缓存，也不保证断网时能够重新构造一份远端详情。provider 请求失败不会因为这次失败主动清空已经写入条目的展示字段，更不会清空 NFO 字段、NFO external IDs、NFO 评分或本地演职员；尚未取得远端数据的新条目则继续保持本地字段与对应的未匹配/失败状态。TMDB 长期复核与到期清理规则以 [`TMDB_INTEGRATION.md`](TMDB_INTEGRATION.md) 为准。

启用该结构后需要重新扫描媒体库以建立来源快照，不需要删除数据库或修改媒体文件。

### 7.1 来源摘要与按需观察

来源诊断接口仅对管理员开放，并继续校验管理员对条目所属媒体库的访问权限。

`GET /api/media-items/{id}/metadata-sources` 返回 external IDs、持久化演职员和来源摘要。每个来源摘要包含稳定 `id`、来源路径、文档类型、schema 版本、锁定兼容标志、选中状态和时间，不读取标准化 payload，不访问文件系统，也不解析 NFO。

`GET /api/media-items/{id}/metadata-sources/{source_id}` 才读取一个来源的标准化 payload，并对该来源路径做一次轻量 NFO 观察。`source_id` 必须属于路径中的条目。观察以条目所属媒体库的根目录为安全边界：来源路径必须位于该根目录内，解析后的真实路径也不能逃逸边界，NFO 文件自身不能是符号链接。该观察只检查文件、读取 XML 并校验预期根类型，不调用 ffprobe、TMDB 或图片下载，也不代表最近一次扫描任务的终态。

- `observation_status = valid`：请求时该路径仍存在，且当前内容可以按持久化的 `document_type` 解析。
- `observation_status = invalid`：路径存在或可见，但当前文件无法安全读取、解析或通过根类型校验；`observation_error_code` 给出稳定原因码。
- `observation_status = missing`：请求时该持久化来源路径不存在；`observation_error_code` 为 `null`。

单源详情中的 `payload` 始终是最近一次成功扫描/刷新后保存的标准化快照，不是本次实时观察得到的临时内容。因此来源刚被写坏或删除时，响应可以同时出现旧 `payload` 与 `invalid` / `missing`；客户端应把前者展示为“最近一次有效内容”，把观察状态展示为当前文件状态。新放入但尚未经过扫描或单条刷新的 NFO 还没有持久化来源记录，不会仅因调用诊断接口自动加入列表。

| `observation_error_code` | 含义 |
| --- | --- |
| `open_failed` | 候选存在，但无法打开 |
| `inspect_failed` | 无法读取候选文件元数据 |
| `not_regular_file` | 候选路径不是普通文件 |
| `too_large` | 打开前检查已超过当前 NFO 大小上限 |
| `read_failed` | 读取过程中发生 I/O 错误 |
| `grew_beyond_limit` | 读取过程中增长并越过硬上限 |
| `invalid_utf8` | 文件不是有效 UTF-8 |
| `forbidden_xml_declaration` | 包含禁止的 DTD 或实体声明 |
| `malformed_xml` | 不是单根良构 XML |
| `unsupported_root` | 根元素不是 `movie`、`tvshow` 或 `episodedetails` |
| `unexpected_root_kind` | 根元素合法，但与该持久化来源的层级不一致 |
| `outside_library_root` | 来源路径或解析后的真实路径位于条目所属媒体库根目录之外 |
| `symlink_not_allowed` | NFO 来源路径自身是符号链接；观察不会跟随该链接读取目标 |
| `secure_open_unavailable` | 当前操作系统没有可验证已打开文件句柄的安全实现；为避免路径检查与打开之间的竞态，服务端拒绝读取 |
| `resource_limit_exceeded` | XML 元素、单字段、演职员、图片、external ID、评分、命名季或多值集合超过结构化资源上限；整份来源无效，不会截断后使用 |
| `unsupported_document_type` | 数据库来源类型不在当前支持范围；属于兼容/数据诊断状态 |

## 8. 扫描流程

```text
权威文件树发现与 sidecar 指纹
→ 读取并校验完整 XML
→ 选择层级正确的本地来源
→ 写入 NFO 快照与 pending 本地投影
→ 使用唯一且类型正确的 direct ID，或按标题规则查询 TMDB
→ TMDB 只补充缺失字段
→ 再应用选中 NFO 的字段所有权
→ 扫描组短事务提交
```

普通扫库、增量文件同步和单条元数据刷新复用相同的 XML 解析、作用域、稳定选源与字段所有权规则。单条刷新会枚举该逻辑条目的全部本地载体：movie / episode 使用自身全部版本，series 使用全部本地季集文件作为 `tvshow.nfo` 查找锚点；候选去重后统一选择，只有确定性代表文件执行 ffprobe。刷新始终优先使用已有的 accepted TMDB binding；冲突 NFO 只保留来源快照，不能触发自动换绑。NFO 的文件名、大小或修改时间变化会改变 `sidecar_fingerprint`，使受影响媒体重新进入本地分析。

## 9. 安全限制

- 电影和单集 NFO 最大 2 MiB；`tvshow.nfo` 最大 4 MiB。
- 文件必须是普通文件和有效 UTF-8；读取过程中继续执行硬上限，防止文件并发增长造成无界内存占用。
- 单份文档最多包含 100,000 个 XML 元素；任何文本节点或属性值不得超过 256 KiB。结构化集合上限为：演员 5,000、导演与编剧 10,000、图片 4,096、external ID 16,384、评分 1,024、命名季/季简介 1,024、多值字段拆分项 16,384。
- 超过任一结构化上限时返回 `resource_limit_exceeded`，整份来源按 `invalid` 处理，不静默截断。已有来源继续保留 last-known-good 快照和投影；首次导入则继续使用其它本地与远端来源。
- XML 解析前拒绝 DTD 和实体声明；解析器不解析外部实体，也不执行网络访问。
- 文档只允许单个 `movie`、`tvshow` 或 `episodedetails` 根元素，并校验根元素与用途一致。
- 空文本被忽略；重复数组按大小写不敏感方式去重并保持首次出现顺序。
- Linux 与 macOS 使用禁止符号链接跳转的描述符相对打开流程，并在已打开句柄上复核真实路径。无法提供等价安全打开语义的操作系统返回 `secure_open_unavailable` 并拒绝读取，避免路径校验与打开之间的竞态。
- 相对本地图片路径必须规范化到 NFO 所在目录内。绝对路径、`..` 或符号链接不得逃逸该边界。
- 本地图片必须具有受支持的扩展名、非空普通文件和匹配的文件头。
- NFO 中的网络图片只允许现有 TMDB 可信 HTTPS 图片端点；不得请求 localhost、私网、`file://`、`plugin://` 或任意第三方地址。
- ffprobe 结果始终优先于 NFO 技术字段。

## 10. 明确不支持

- 创建、保存、格式化或写回 NFO。
- URL-only 和 combination NFO。
- Kodi v21 及更早版本的堆叠多集 NFO。
- Kodi v22 的一文件多逻辑集 NFO / artwork 映射。
- `season.nfo` 和季级 NFO 投影。
- 从 `showtitle` 反向绑定父剧；父剧身份只来自 `tvshow.nfo`、已接受 binding 或正常系列匹配。
- 从 episode provider ID 推导或替换 series provider ID。
- 导入 `watched`、`playcount`、`lastplayed`、`resume`、`isuserfavorite`、`userrating` 或 `episodebookmark` 等用户状态。
- 导入 `episodenumberend` 或用 NFO 把一个物理文件映射为多个逻辑单集。
- 使用 `fileinfo/streamdetails` 覆盖真实文件探测结果。
- 执行插件 URL、脚本、外部实体或 NFO 中任意网络请求。
- 原样保存未知 XML、私有扩展节点或原始 XML 文本。

## 11. 验收用例

- 电影同名 NFO 与 `movie.nfo` 同时存在时，选择有效的同名 `movie` 文档。
- `tvshow.nfo` 的标题、简介和 TMDB ID 只作用于 series；单集 NFO 的标题、简介和 ID 只作用于 episode。
- 单集 NFO 的 TMDB ID 不会成为父剧 binding，也不会发起 series direct lookup。
- 合法部分 NFO 保留其非空字段，TMDB 只补齐缺失字段。
- NFO TMDB ID 只有在 direct lookup 返回类型正确的详情后才成为 binding。
- 已有 accepted binding 时，冲突 NFO 不换绑、不进入公共投影，但保留来源快照并产生诊断。
- 多版本包含多份 NFO 时，选择结果不随文件系统遍历顺序变化，也不跨文档逐字段拼接。
- 已成功解析的 NFO 被临时写坏时继续使用 last-known-good；权威发现确认文件删除后才移除快照。
- 错误根、URL-only、DTD、外部实体、超限文件和多顶层 episode 文档均被拒绝，媒体文件仍可按其它来源入库。
- NFO `runtime`、`fileinfo`、播放状态和收藏不会覆盖 ffprobe 或用户级数据。

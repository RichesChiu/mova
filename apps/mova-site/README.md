# MOVA 官方网站

这是 MOVA 自托管媒体服务的官方网站，源代码位于主仓库的 `apps/mova-site`，通过 GitHub Actions 部署到 GitHub Pages。

访问地址：[https://mova.hk/](https://mova.hk/)

## 本地验证

从仓库根目录执行：

```bash
npm --prefix apps/mova-site ci
npm --prefix apps/mova-site run check:api-docs
npm --prefix apps/mova-site run lint
npm --prefix apps/mova-site run typecheck
npm --prefix apps/mova-site run build
```

`docs/API.md` 是接口契约来源，官网的接口索引位于 `src/data/apiDocs.ts`。修改接口或官网 API 页面时，必须保持两者同步。

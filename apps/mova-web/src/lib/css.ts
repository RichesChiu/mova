export const cssBackgroundImage = (imageUrl: string) =>
  `url("${imageUrl.replace(/["\\]/g, '\\$&')}")`

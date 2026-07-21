export const HOME_LIBRARY_LIMIT = 5

export const getVisibleHomeLibraries = <Item>(items: Item[]) => items.slice(0, HOME_LIBRARY_LIMIT)

export const shouldShowAllHomeLibraries = (totalLibraryCount: number) =>
  totalLibraryCount > HOME_LIBRARY_LIMIT

export const shouldRenderHomeContinueWatching = ({
  hasError,
  isLoading,
  itemCount,
}: {
  hasError: boolean
  isLoading: boolean
  itemCount: number
}) => isLoading || hasError || itemCount > 0

export const shouldRenderHomeRecentlyAdded = ({
  hasError,
  isLoading,
  libraryCount,
}: {
  hasError: boolean
  isLoading: boolean
  libraryCount: number
}) => isLoading || hasError || libraryCount > 0

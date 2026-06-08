import { useEffect, useState } from 'react'
import type { ServerMediaDirectoryNode } from '../../api/types'

interface MediaDirectoryTreeProps {
  tree: ServerMediaDirectoryNode
  selectedPath: string
  onSelect: (path: string) => void
}

interface MediaDirectoryTreeNodeProps {
  depth: number
  expandedPaths: Set<string>
  node: ServerMediaDirectoryNode
  onSelect: (path: string) => void
  onToggle: (path: string) => void
  selectedPath: string
}

const collectAncestorPaths = (
  node: ServerMediaDirectoryNode,
  targetPath: string,
  ancestors: string[] = [],
): string[] | null => {
  if (node.path === targetPath) {
    return ancestors
  }

  for (const child of node.children) {
    const childAncestors = collectAncestorPaths(child, targetPath, [...ancestors, node.path])
    if (childAncestors) {
      return childAncestors
    }
  }

  return null
}

const MediaDirectoryTreeNode = ({
  depth,
  expandedPaths,
  node,
  onSelect,
  onToggle,
  selectedPath,
}: MediaDirectoryTreeNodeProps) => {
  const hasChildren = node.children.length > 0
  const isExpanded = expandedPaths.has(node.path)
  const isSelected = node.path === selectedPath

  return (
    <li className="media-tree__item">
      <div
        className={isSelected ? 'media-tree__row media-tree__row--selected' : 'media-tree__row'}
        style={{ paddingLeft: `${depth * 16 + 6}px` }}
      >
        {hasChildren ? (
          <button
            aria-expanded={isExpanded}
            aria-label={isExpanded ? `Collapse ${node.name}` : `Expand ${node.name}`}
            className="media-tree__toggle"
            onClick={() => onToggle(node.path)}
            type="button"
          >
            <svg
              aria-hidden="true"
              className={
                isExpanded
                  ? 'media-tree__toggle-icon media-tree__toggle-icon--open'
                  : 'media-tree__toggle-icon'
              }
              fill="none"
              viewBox="0 0 16 16"
            >
              <path
                d="M5.5 3.5L10 8L5.5 12.5"
                stroke="currentColor"
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="1.8"
              />
            </svg>
          </button>
        ) : (
          <span aria-hidden="true" className="media-tree__leaf-spacer" />
        )}

        <button
          className="media-tree__button"
          onClick={() => onSelect(node.path)}
          title={node.path}
          type="button"
        >
          <span aria-hidden="true" className="media-tree__folder-icon">
            <svg aria-hidden="true" fill="none" focusable="false" viewBox="0 0 16 16">
              <path
                d="M2.5 4.75C2.5 4.06 3.06 3.5 3.75 3.5H6.1C6.48 3.5 6.84 3.67 7.07 3.96L7.43 4.42C7.67 4.73 8.03 4.9 8.41 4.9H12.25C12.94 4.9 13.5 5.46 13.5 6.15V11.25C13.5 11.94 12.94 12.5 12.25 12.5H3.75C3.06 12.5 2.5 11.94 2.5 11.25V4.75Z"
                stroke="currentColor"
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="1.4"
              />
            </svg>
          </span>
          <span className="media-tree__name">{node.name}</span>
        </button>
      </div>

      {hasChildren && isExpanded ? (
        <ul className="media-tree__list">
          {node.children.map((child) => (
            <MediaDirectoryTreeNode
              depth={depth + 1}
              expandedPaths={expandedPaths}
              key={child.path}
              node={child}
              onSelect={onSelect}
              onToggle={onToggle}
              selectedPath={selectedPath}
            />
          ))}
        </ul>
      ) : null}
    </li>
  )
}

export const MediaDirectoryTree = ({ tree, selectedPath, onSelect }: MediaDirectoryTreeProps) => {
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(() => new Set())

  useEffect(() => {
    // 选中项变化后，自动展开它所在的父路径，避免树切换后选中节点被折叠藏起来。
    const ancestorPaths = collectAncestorPaths(tree, selectedPath)
    if (!ancestorPaths) {
      return
    }

    setExpandedPaths((current) => {
      const next = new Set(current)
      ancestorPaths.forEach((path) => {
        next.add(path)
      })
      return next
    })
  }, [selectedPath, tree])

  const togglePath = (path: string) => {
    setExpandedPaths((current) => {
      const next = new Set(current)
      if (next.has(path)) {
        next.delete(path)
      } else {
        next.add(path)
      }
      return next
    })
  }

  return (
    <ul className="media-tree__list media-tree__list--root">
      <MediaDirectoryTreeNode
        depth={0}
        expandedPaths={expandedPaths}
        node={tree}
        onSelect={onSelect}
        onToggle={togglePath}
        selectedPath={selectedPath}
      />
    </ul>
  )
}

export type TooltipPlacement = 'bottom' | 'top'

interface TooltipPositionInput {
  gap: number
  margin: number
  preferredPlacement: TooltipPlacement
  tooltipHeight: number
  tooltipWidth: number
  triggerBottom: number
  triggerCenterX: number
  triggerTop: number
  viewportHeight: number
  viewportWidth: number
}

export interface TooltipPosition {
  arrowLeft: number
  left: number
  placement: TooltipPlacement
  top: number
}

const clamp = (value: number, minimum: number, maximum: number) =>
  Math.max(minimum, Math.min(value, maximum))

export const resolveTooltipPosition = ({
  gap,
  margin,
  preferredPlacement,
  tooltipHeight,
  tooltipWidth,
  triggerBottom,
  triggerCenterX,
  triggerTop,
  viewportHeight,
  viewportWidth,
}: TooltipPositionInput): TooltipPosition => {
  const availableAbove = triggerTop - margin - gap
  const availableBelow = viewportHeight - triggerBottom - margin - gap
  const canPlaceAbove = availableAbove >= tooltipHeight
  const canPlaceBelow = availableBelow >= tooltipHeight

  const placement =
    preferredPlacement === 'top'
      ? canPlaceAbove || (!canPlaceBelow && availableAbove >= availableBelow)
        ? 'top'
        : 'bottom'
      : canPlaceBelow || (!canPlaceAbove && availableBelow >= availableAbove)
        ? 'bottom'
        : 'top'

  const maxLeft = Math.max(margin, viewportWidth - tooltipWidth - margin)
  const left = clamp(triggerCenterX - tooltipWidth / 2, margin, maxLeft)
  const idealTop = placement === 'top' ? triggerTop - tooltipHeight - gap : triggerBottom + gap
  const maxTop = Math.max(margin, viewportHeight - tooltipHeight - margin)
  const top = clamp(idealTop, margin, maxTop)
  const arrowLeft = clamp(triggerCenterX - left, 18, Math.max(18, tooltipWidth - 18))

  return {
    arrowLeft,
    left,
    placement,
    top,
  }
}

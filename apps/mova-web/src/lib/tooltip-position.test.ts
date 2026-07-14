import { describe, expect, it } from 'vitest'
import { resolveTooltipPosition } from './tooltip-position'

const baseInput = {
  gap: 12,
  margin: 12,
  preferredPlacement: 'top' as const,
  tooltipHeight: 80,
  tooltipWidth: 240,
  triggerBottom: 240,
  triggerCenterX: 400,
  triggerTop: 210,
  viewportHeight: 800,
  viewportWidth: 1000,
}

describe('resolveTooltipPosition', () => {
  it('places the tooltip above the trigger by default', () => {
    expect(resolveTooltipPosition(baseInput)).toEqual({
      arrowLeft: 120,
      left: 280,
      placement: 'top',
      top: 118,
    })
  })

  it('moves the tooltip below when the top has no room', () => {
    expect(
      resolveTooltipPosition({
        ...baseInput,
        triggerBottom: 70,
        triggerTop: 40,
      }),
    ).toMatchObject({
      placement: 'bottom',
      top: 82,
    })
  })

  it('uses the roomier side and keeps the surface inside the viewport', () => {
    expect(
      resolveTooltipPosition({
        ...baseInput,
        tooltipHeight: 500,
        triggerBottom: 410,
        triggerTop: 390,
        viewportHeight: 620,
      }),
    ).toMatchObject({
      placement: 'top',
      top: 12,
    })
  })

  it('clamps the surface horizontally while keeping the arrow on the trigger', () => {
    expect(
      resolveTooltipPosition({
        ...baseInput,
        triggerCenterX: 24,
      }),
    ).toMatchObject({
      arrowLeft: 18,
      left: 12,
    })
  })
})

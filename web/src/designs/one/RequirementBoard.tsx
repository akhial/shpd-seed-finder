import { useEffect, useRef, useState } from 'react'
import type { CSSProperties, KeyboardEvent as ReactKeyboardEvent, PointerEvent as ReactPointerEvent, ReactNode } from 'react'
import { displayItemName, sourceLabel } from '../../lib/catalog'
import { effectGlows } from '../../lib/glow'
import type { Glow } from '../../lib/glow'
import { PlusIcon, XIcon } from '../../lib/icons'
import { STACK_MAX, isAnyEnchantment, levelSumCapacity, maxUpgradeFor, requirementFamily, validateRequirement } from '../../lib/query'
import type { RequirementState } from '../../lib/wasm/types'
import { Sprite } from './parts'
import {
  boardItems,
  copyDepthOf,
  detach,
  joinAlternatives,
  removeItem,
  removeMember,
  setStackCount,
  setStackTotal,
  stackCount,
} from './relations'
import type { BoardItem } from './relations'
import { effectLabel, requirementSprite, requirementTitle } from './summary'

/**
 * The requirement board: every requirement is a chip; drop one chip onto
 * another for an either/or cluster, drag a chip out of its cluster to make
 * it standalone again. Everything else is a property of the chip itself:
 * a stack badge (×N / ≤N) for "more of the same kind", and a Σ badge for a
 * stack whose items count their levels towards one total.
 */

const DRAG_THRESHOLD = 5
const LONG_PRESS_MS = 480

const WILDCARD_SHORT: Record<string, string> = {
  weapon: 'Any weapon',
  melee_weapon: 'Any melee',
  thrown_weapon: 'Any thrown',
  armor: 'Any armor',
  wand: 'Any wand',
  ring: 'Any ring',
}

/** The short name a chip shows: the item, or its wildcard family. */
export const chipName = (requirement: RequirementState): string => {
  if (requirement.item) return displayItemName(requirement.item)
  return requirement.kind ? WILDCARD_SHORT[requirement.kind] ?? requirement.kind : 'Any item'
}

/** A qualifier beside a chip's name; the upgrade is tinted apart from the rest. */
export interface ChipTag { text: string; upgrade?: true }

/** The tiny qualifiers beside a chip's name: tier, upgrade, floor. */
export function chipTags(requirement: RequirementState): ChipTag[] {
  const tags: ChipTag[] = []
  const { tier, upgrade } = requirement
  if (!requirement.item && tier.mode === 'exact') tags.push({ text: `T${tier.value}` })
  if (!requirement.item && tier.mode === 'at_least') tags.push({ text: `T${tier.value}+` })
  if (!requirement.item && tier.mode === 'at_most') tags.push({ text: `T≤${tier.value}` })
  if (upgrade.mode === 'exact') tags.push({ text: `+${upgrade.value}`, upgrade: true })
  if (upgrade.mode === 'at_least') tags.push({ text: `+${upgrade.value}↑`, upgrade: true })
  if (requirement.maxDepth !== undefined) tags.push({ text: `F≤${requirement.maxDepth}` })
  return tags
}

/** How wide one effect's colour holds the badge's ring, and its blend into the next. */
const SWEEP_BAND = 28
const SWEEP_FADE = 10

/**
 * The many-effects badge: a ring that sweeps through every selected effect's
 * colour, each holding for one pulse cycle (2 × its period) so the badge beats
 * at the cadence the sprites glow at. The colours are laid out as one tile that
 * ends where it began and the ring scrolls by exactly that tile, so the loop
 * never seams.
 */
function effectSweepCss(glows: Glow[]): CSSProperties {
  const width = glows.length * SWEEP_BAND
  const stops = glows.flatMap((glow, index) => [
    `${glow.color} ${index * SWEEP_BAND}px`,
    `${glow.color} ${(index + 1) * SWEEP_BAND - SWEEP_FADE}px`,
  ])
  stops.push(`${glows[0].color} ${width}px`)
  return {
    '--d1-sweep': `linear-gradient(90deg, ${stops.join(', ')})`,
    '--d1-sweep-width': `${width}px`,
    '--d1-sweep-shift': `${-width}px`,
    animationDuration: `${glows.reduce((total, glow) => total + glow.period * 2, 0)}s`,
  } as CSSProperties
}

type DropTarget =
  | { kind: 'chip'; index: number }
  | { kind: 'cluster'; group: number }
  | { kind: 'delete' }
  | { kind: 'board' }

interface DragState {
  source: number
  x: number
  y: number
  over: DropTarget | null
}

interface MenuState { item: BoardItem; index: number; x: number; y: number }
interface PickState { source: number }
interface StepperState { key: string; which: 'count' | 'total' }

/** What the editor needs to know about the chip's stack. */
export interface StackShape {
  count: number
  total?: number
  /** The floor limit the extra copies share, when they carry one. */
  copyDepth?: number
  /** A cluster member's stack belongs to the cluster, not the editor. */
  inCluster: boolean
}

export function RequirementBoard({
  requirements,
  onChange,
  onEdit,
  onAdd,
}: {
  requirements: RequirementState[]
  onChange: (requirements: RequirementState[]) => void
  onEdit: (index: number, stack: StackShape) => void
  onAdd: () => void
}) {
  const wrapRef = useRef<HTMLDivElement>(null)
  const [drag, setDrag] = useState<DragState | null>(null)
  const [menu, setMenu] = useState<MenuState | null>(null)
  const [pick, setPick] = useState<PickState | null>(null)
  const [stepper, setStepper] = useState<StepperState | null>(null)
  const [hovered, setHoveredState] = useState<{ index: number; left: number; top: number } | null>(null)
  const pressRef = useRef<{ index: number; x: number; y: number; timer: number | undefined; dragging: boolean } | null>(null)
  const dragRef = useRef<DragState | null>(null)

  const items = boardItems(requirements)
  const itemOf = (index: number): BoardItem | undefined => items.find((item) => item.members.includes(index))
  const stackOf = (item: BoardItem): StackShape => ({
    count: stackCount(item),
    total: item.total,
    copyDepth: copyDepthOf(requirements, item),
    inCluster: item.cluster !== undefined,
  })

  const hoveredIndex = hovered?.index ?? null
  const setHovered = (index: number | null, element?: HTMLElement) => {
    if (index === null || !element) {
      setHoveredState(null)
      return
    }
    const rect = element.getBoundingClientRect()
    setHoveredState({ index, left: Math.min(rect.left, window.innerWidth - 300), top: rect.bottom + 8 })
  }

  // ---- drag -----------------------------------------------------------------

  const targetAt = (x: number, y: number): DropTarget | null => {
    const element = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-drop]')
    if (!element || !wrapRef.current?.contains(element)) return null
    const kind = element.dataset.drop
    if (kind === 'chip') return { kind: 'chip', index: Number(element.dataset.chip) }
    if (kind === 'cluster') return { kind: 'cluster', group: Number(element.dataset.group) }
    if (kind === 'delete') return { kind: 'delete' }
    return { kind: 'board' }
  }

  const updateDrag = (next: DragState | null) => {
    dragRef.current = next
    setDrag(next)
  }

  const completeDrop = (state: DragState) => {
    const { source, over } = state
    if (!over) return
    const current = requirements[source]
    let next: RequirementState[] | undefined
    if (over.kind === 'chip') {
      if (over.index === source) return
      next = joinAlternatives(requirements, source, over.index)
    } else if (over.kind === 'cluster') {
      if (current.alternativeGroup === over.group) return
      const target = items.find((entry) => entry.cluster === over.group)
      if (target) next = joinAlternatives(requirements, source, target.members[0])
    } else if (over.kind === 'delete') {
      const item = itemOf(source)
      if (item) next = item.cluster !== undefined ? removeMember(requirements, source) : removeItem(requirements, item)
    } else if (current.alternativeGroup !== undefined) {
      next = detach(requirements, source)
    }
    if (next && next !== requirements) onChange(next)
  }

  const onChipPointerDown = (index: number) => (event: ReactPointerEvent<HTMLElement>) => {
    if (event.button !== 0) return
    if ((event.target as HTMLElement).closest('[data-no-drag]')) return
    event.currentTarget.setPointerCapture(event.pointerId)
    const timer = event.pointerType === 'mouse' ? undefined : window.setTimeout(() => {
      const press = pressRef.current
      if (!press || press.dragging) return
      pressRef.current = null
      const item = itemOf(index)
      if (item) setMenu({ item, index, x: press.x, y: press.y })
    }, LONG_PRESS_MS)
    pressRef.current = { index, x: event.clientX, y: event.clientY, timer, dragging: false }
  }

  const onChipPointerMove = (event: ReactPointerEvent<HTMLElement>) => {
    const press = pressRef.current
    if (!press) return
    if (!press.dragging) {
      if (Math.hypot(event.clientX - press.x, event.clientY - press.y) < DRAG_THRESHOLD) return
      window.clearTimeout(press.timer)
      press.dragging = true
      setMenu(null)
      setHovered(null)
      setPick(null)
      setStepper(null)
    }
    updateDrag({ source: press.index, x: event.clientX, y: event.clientY, over: targetAt(event.clientX, event.clientY) })
  }

  const editChip = (index: number) => {
    const item = itemOf(index)
    onEdit(index, item ? stackOf(item) : { count: 1, inCluster: false })
  }

  const onChipPointerUp = (event: ReactPointerEvent<HTMLElement>) => {
    const press = pressRef.current
    pressRef.current = null
    if (!press) return
    window.clearTimeout(press.timer)
    if (press.dragging) {
      const state = dragRef.current
      updateDrag(null)
      if (state) completeDrop({ ...state, over: targetAt(event.clientX, event.clientY) })
      return
    }
    if (pick) {
      if (pick.source !== press.index) onChange(joinAlternatives(requirements, pick.source, press.index))
      setPick(null)
      return
    }
    editChip(press.index)
  }

  const onChipPointerCancel = () => {
    const press = pressRef.current
    pressRef.current = null
    if (press) window.clearTimeout(press.timer)
    updateDrag(null)
  }

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      if (dragRef.current) {
        pressRef.current = null
        updateDrag(null)
      }
      setMenu(null)
      setPick(null)
      setStepper(null)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const onChipKeyDown = (index: number) => (event: ReactKeyboardEvent<HTMLElement>) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      if (pick) {
        if (pick.source !== index) onChange(joinAlternatives(requirements, pick.source, index))
        setPick(null)
      } else editChip(index)
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault()
      const item = itemOf(index)
      if (!item) return
      onChange(item.cluster !== undefined ? removeMember(requirements, index) : removeItem(requirements, item))
    } else if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10') || event.key === '.') {
      event.preventDefault()
      const rect = event.currentTarget.getBoundingClientRect()
      const item = itemOf(index)
      if (item) setMenu({ item, index, x: rect.left, y: rect.bottom })
    }
  }

  // ---- rendering ---------------------------------------------------------------

  const dropClass = (target: DropTarget): string => {
    const over = drag?.over
    if (!over) return ''
    const same = over.kind === target.kind
      && (over.kind === 'chip' ? over.index === (target as { index: number }).index
        : over.kind === 'cluster' ? over.group === (target as { group: number }).group
          : true)
    if (!same) return ''
    if (over.kind === 'board') {
      return drag && requirements[drag.source].alternativeGroup !== undefined ? ' d1-drop-detach' : ''
    }
    return ' d1-drop-alternative'
  }

  const renderChip = (index: number, inCluster: boolean) => {
    const requirement = requirements[index]
    const errors = validateRequirement(requirement)
    const classes = ['d1-chip']
    if (drag?.source === index) classes.push('d1-chip-dragging')
    if (errors.length > 0) classes.push('d1-chip-error')
    if (pick) classes.push(pick.source === index ? 'd1-chip-pick-source' : 'd1-chip-pickable')
    const glows = effectGlows(requirement.effect)
    const glow = glows[0] ?? null
    const effect = effectLabel(requirement)
    const item = itemOf(index)
    const showBadges = item !== undefined && !inCluster
    return (
      <div
        key={index}
        role="button"
        tabIndex={0}
        className={classes.join(' ') + dropClass({ kind: 'chip', index })}
        data-drop="chip"
        data-chip={index}
        aria-label={requirementTitle(requirement)}
        onPointerDown={onChipPointerDown(index)}
        onPointerMove={onChipPointerMove}
        onPointerUp={onChipPointerUp}
        onPointerCancel={onChipPointerCancel}
        onKeyDown={onChipKeyDown(index)}
        onContextMenu={(event) => {
          event.preventDefault()
          const owner = itemOf(index)
          if (owner) setMenu({ item: owner, index, x: event.clientX, y: event.clientY })
        }}
        onMouseEnter={(event) => setHovered(index, event.currentTarget)}
        onMouseLeave={() => { if (hoveredIndex === index) setHovered(null) }}
        onFocus={(event) => setHovered(index, event.currentTarget)}
        onBlur={() => { if (hoveredIndex === index) setHovered(null) }}
      >
        <Sprite index={requirementSprite(requirement)} size={18} glow={glows} />
        <span className="d1-chip-name">{chipName(requirement)}</span>
        {chipTags(requirement).map((tag) => (
          <span key={tag.text} className={tag.upgrade ? 'd1-chip-tag d1-chip-tag-up' : 'd1-chip-tag'}>{tag.text}</span>
        ))}
        {effect && (glows.length > 1 ? (
          <span className="d1-chip-effect d1-chip-effect-multi" style={effectSweepCss(glows)} title={effect}>
            {glows.length}
          </span>
        ) : (
          <span
            className={`d1-chip-effect${isAnyEnchantment(requirement.effect) ? ' d1-chip-effect-any' : glow ? '' : ' d1-chip-effect-curse'}`}
            style={glow ? { background: glow.color } : undefined}
            title={effect}
          />
        ))}
        {requirement.uncursed && <span className="d1-chip-tag d1-chip-tag-soft" title="Uncursed">✓</span>}
        {showBadges && item && renderBadges(item)}
      </div>
    )
  }

  /** The stack (×N / ≤N) and combined-level (Σ) badges with their steppers. */
  const renderBadges = (item: BoardItem): ReactNode => {
    const count = stackCount(item)
    const anchor = requirements[item.members[0]]
    const capacity = item.total !== undefined
      ? levelSumCapacity([anchor, ...item.extras.map((index) => requirements[index])])
      : count * (maxUpgradeFor(requirementFamily(anchor)) + 1)
    const editingCount = stepper?.key === item.key && stepper.which === 'count'
    const editingTotal = stepper?.key === item.key && stepper.which === 'total'
    return (
      <>
        {editingCount ? (
          <span className="d1-stack-edit" role="group" aria-label="How many" onPointerDown={(event) => event.stopPropagation()} data-no-drag>
            <button type="button" aria-label="One fewer" disabled={count <= 1} onClick={() => onChange(setStackCount(requirements, item, count - 1))}>−</button>
            <span className="d1-stack-badge">{item.total !== undefined ? `≤${count}` : `×${count}`}</span>
            <button type="button" aria-label="One more" disabled={count >= STACK_MAX} onClick={() => onChange(setStackCount(requirements, item, count + 1))}>+</button>
            <button type="button" className="d1-stack-done" aria-label="Done" onClick={() => setStepper(null)}>✓</button>
          </span>
        ) : (count > 1 || editingTotal) && (
          <button
            type="button"
            className="d1-stack-badge d1-stack-badge-btn"
            data-no-drag
            title={item.total !== undefined ? `Up to ${count} items` : `${count} of the same kind`}
            onPointerDown={(event) => event.stopPropagation()}
            onClick={() => setStepper({ key: item.key, which: 'count' })}
          >
            {item.total !== undefined ? `≤${count}` : `×${count}`}
          </button>
        )}
        {editingTotal ? (
          <span className="d1-stack-edit d1-stack-edit-total" role="group" aria-label="Combined level" onPointerDown={(event) => event.stopPropagation()} data-no-drag>
            <button type="button" aria-label="Lower total" onClick={() => onChange(item.total === 1 ? setStackTotal(requirements, item, undefined) : setStackTotal(requirements, item, (item.total ?? 2) - 1))}>−</button>
            <span className="d1-stack-badge d1-stack-badge-total">Σ ≥ {item.total ?? 0}</span>
            <button type="button" aria-label="Raise total" disabled={(item.total ?? 0) >= capacity} onClick={() => onChange(setStackTotal(requirements, item, (item.total ?? 0) + 1))}>+</button>
            <button type="button" className="d1-stack-done" aria-label="Done" onClick={() => setStepper(null)}>✓</button>
          </span>
        ) : item.total !== undefined && (
          <button
            type="button"
            className="d1-stack-badge d1-stack-badge-total d1-stack-badge-btn"
            data-no-drag
            title={`Levels add to at least ${item.total} (a +0 item counts 1)`}
            onPointerDown={(event) => event.stopPropagation()}
            onClick={() => setStepper({ key: item.key, which: 'total' })}
          >
            Σ ≥ {item.total}
          </button>
        )}
      </>
    )
  }

  const renderItem = (item: BoardItem): ReactNode => {
    if (item.cluster === undefined) return renderChip(item.members[0], false)
    return (
      <div
        key={item.key}
        className={`d1-cluster${dropClass({ kind: 'cluster', group: item.cluster })}`}
        data-drop="cluster"
        data-group={item.cluster}
        role="group"
        aria-label={`Any of ${item.members.length}`}
      >
        {item.members.map((index, position) => (
          <span key={index} className="d1-cluster-member">
            {position > 0 && <span className="d1-cluster-or" aria-hidden="true">or</span>}
            {renderChip(index, true)}
          </span>
        ))}
        {renderBadges(item)}
      </div>
    )
  }

  const dragSource = drag ? requirements[drag.source] : null
  const statusLine = pick ? 'Either/or with… choose a chip' : null

  return (
    <div ref={wrapRef} className="d1-board-wrap">
      <div
        className={`d1-board${drag ? ' d1-board-dragging' : ''}${dropClass({ kind: 'board' })}`}
        data-drop="board"
        onMouseDown={(event) => {
          // The steppers stop pointerdown, but this compatibility mousedown
          // still bubbles — closing the stepper here would unmount its
          // buttons before their click can fire.
          if ((event.target as HTMLElement).closest('[data-no-drag]')) return
          setMenu(null)
          setStepper(null)
        }}
      >
        {items.map(renderItem)}
        <button type="button" className="d1-chip d1-chip-add" onClick={onAdd} title="Add a requirement">
          <PlusIcon size={13} />
          <span>Add</span>
        </button>
      </div>
      {drag && (
        <div
          className={`d1-delete-zone${drag.over?.kind === 'delete' ? ' d1-delete-zone-over' : ''}`}
          data-drop="delete"
        >
          <XIcon size={12} />
          <span>drop to remove</span>
        </div>
      )}
      {statusLine && <div className="d1-board-status d1-mono" aria-live="polite">{statusLine}</div>}
      {hovered && !drag && !menu && requirements[hovered.index] && (
        <ChipPopover
          requirements={requirements}
          index={hovered.index}
          item={itemOf(hovered.index)}
          style={{ left: hovered.left, top: hovered.top }}
        />
      )}
      {drag && dragSource && (
        <div className="d1-chip d1-chip-ghost" style={{ left: drag.x, top: drag.y }} aria-hidden="true">
          <Sprite index={requirementSprite(dragSource)} size={18} />
          <span className="d1-chip-name">{chipName(dragSource)}</span>
          {(drag.over?.kind === 'chip' || drag.over?.kind === 'cluster') && <span className="d1-chip-ghost-tag d1-ghost-alternative">or</span>}
          {drag.over?.kind === 'delete' && <span className="d1-chip-ghost-tag d1-ghost-delete">remove</span>}
        </div>
      )}
      {menu && (
        <ChipMenu
          state={menu}
          requirements={requirements}
          onClose={() => setMenu(null)}
          onEdit={() => { setMenu(null); editChip(menu.index) }}
          onPick={() => { setMenu(null); setPick({ source: menu.index }) }}
          onCount={(count) => onChange(setStackCount(requirements, menu.item, count))}
          onTotal={() => {
            setMenu(null)
            if (menu.item.total === undefined) {
              onChange(setStackTotal(requirements, menu.item, Math.max(1, stackCount(menu.item))))
            } else {
              onChange(setStackTotal(requirements, menu.item, undefined))
            }
          }}
          onDetach={() => { setMenu(null); onChange(detach(requirements, menu.index)) }}
          onRemove={() => {
            setMenu(null)
            onChange(menu.item.cluster !== undefined ? removeMember(requirements, menu.index) : removeItem(requirements, menu.item))
          }}
        />
      )}
    </div>
  )
}

/** The detail card under a hovered or focused chip. */
function ChipPopover({ requirements, index, item, style }: {
  requirements: RequirementState[]
  index: number
  item: BoardItem | undefined
  style: CSSProperties
}) {
  const requirement = requirements[index]
  const errors = validateRequirement(requirement)
  const lines: string[] = []
  if (requirement.upgrade.mode === 'exact') lines.push(`exactly +${requirement.upgrade.value}`)
  else if (requirement.upgrade.mode === 'at_least') lines.push(`+${requirement.upgrade.value} or higher`)
  else if (item?.total === undefined) lines.push('any upgrade')
  const effect = effectLabel(requirement)
  if (effect) lines.push(effect)
  if (requirement.uncursed) lines.push('uncursed')
  if (requirement.source) lines.push(sourceLabel(requirement.source))
  if (requirement.maxDepth !== undefined) lines.push(`floors 1–${requirement.maxDepth}`)
  const relations: { glyph: string; text: string }[] = []
  if (requirement.alternativeGroup !== undefined) {
    const peers = requirements
      .filter((other) => other !== requirement && other.alternativeGroup === requirement.alternativeGroup)
      .map(chipName)
    relations.push({ glyph: 'or', text: peers.join(', ') })
  }
  if (item && item.total !== undefined) {
    relations.push({ glyph: 'Σ', text: `up to ${stackCount(item)} — levels add to ≥ ${item.total}` })
  } else if (item && stackCount(item) > 1) {
    // The chip's own bounds (+3, F≤4) describe one copy, not the extras.
    const depths = [...new Set(item.extras.map((extra) => requirements[extra].maxDepth))]
    const floors = depths.length > 1 ? 'own floor limits'
      : depths[0] !== undefined ? `floors 1–${depths[0]}` : 'any floor'
    relations.push({ glyph: '×', text: `${stackCount(item)} of the same kind — the extra copies: any upgrade, ${floors}` })
  }
  return (
    <div className="d1-chip-pop" role="tooltip" style={style}>
      <div className="d1-chip-pop-title">{requirementTitle(requirement)}</div>
      {lines.length > 0 && <div className="d1-chip-pop-sub">{lines.join(' · ')}</div>}
      {relations.map((relation) => (
        <div key={relation.glyph} className="d1-chip-pop-rel">
          <span className="d1-chip-pop-glyph">{relation.glyph}</span>
          <span>{relation.text}</span>
        </div>
      ))}
      {errors.length > 0 && <div className="d1-chip-pop-error">{errors[0]}</div>}
    </div>
  )
}

/** The chip's context menu: the gestures as words, for keyboard and touch. */
function ChipMenu({
  state,
  requirements,
  onClose,
  onEdit,
  onPick,
  onCount,
  onTotal,
  onDetach,
  onRemove,
}: {
  state: MenuState
  requirements: RequirementState[]
  onClose: () => void
  onEdit: () => void
  onPick: () => void
  onCount: (count: number) => void
  onTotal: () => void
  onDetach: () => void
  onRemove: () => void
}) {
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>('button')?.focus()
    const onDown = (event: MouseEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose()
    }
    window.addEventListener('mousedown', onDown)
    return () => window.removeEventListener('mousedown', onDown)
  }, [onClose])
  const left = Math.min(state.x, window.innerWidth - 230)
  const top = Math.min(state.y, window.innerHeight - 260)
  const { item } = state
  const count = stackCount(item)
  const anchor = requirements[item.members[0]]
  const inCluster = item.cluster !== undefined
  const canPick = requirements.length > 1
  const canTotal = !inCluster && anchor.item !== undefined && count > 1
  return (
    <div ref={ref} className="d1-chip-menu" role="menu" style={{ left, top }}>
      <button type="button" role="menuitem" onClick={onEdit}>Edit…</button>
      {canPick && <button type="button" role="menuitem" onClick={onPick}><b>or</b>Either/or with…</button>}
      <span className="d1-chip-menu-rule" />
      <div className="d1-chip-menu-stepper" role="group" aria-label="How many">
        <span><b>×</b>How many</span>
        <span className="d1-chip-menu-count">
          <button type="button" aria-label="One fewer" disabled={count <= 1} onClick={() => onCount(count - 1)}>−</button>
          <span className="d1-mono">{count}</span>
          <button type="button" aria-label="One more" disabled={count >= STACK_MAX} onClick={() => onCount(count + 1)}>+</button>
        </span>
      </div>
      {canTotal && (
        <button type="button" role="menuitem" onClick={onTotal}>
          <b>Σ</b>{item.total === undefined ? 'Count levels together' : 'Stop counting levels'}
        </button>
      )}
      {inCluster && (
        <>
          <span className="d1-chip-menu-rule" />
          <button type="button" role="menuitem" onClick={onDetach}>On its own</button>
        </>
      )}
      <span className="d1-chip-menu-rule" />
      <button type="button" role="menuitem" className="d1-chip-menu-danger" onClick={onRemove}>Remove</button>
    </div>
  )
}

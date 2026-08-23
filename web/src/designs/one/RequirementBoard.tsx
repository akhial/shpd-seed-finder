import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { CSSProperties, KeyboardEvent as ReactKeyboardEvent, PointerEvent as ReactPointerEvent, ReactNode } from 'react'
import { displayItemName, sourceLabel } from '../../lib/catalog'
import { effectGlow } from '../../lib/glow'
import { PlusIcon, XIcon } from '../../lib/icons'
import { effectNamesOf, isAnyEnchantment, upgradeSumCapacity, validateRequirement } from '../../lib/query'
import type { RequirementState } from '../../lib/wasm/types'
import { Sprite } from './parts'
import {
  boardItems,
  detach,
  joinAlternatives,
  joinBundle,
  relate,
  removeAt,
  setBundleTotal,
  unlinkIdentity,
} from './relations'
import type { BoardItem, Relation } from './relations'
import { effectLabel, requirementKind, requirementSprite, requirementTitle } from './summary'

/**
 * The requirement board: every requirement is a chip, and relationships are
 * made by moving chips onto each other.
 *
 *   drop on a chip              → either/or cluster   (one of these fills the slot)
 *   ⇧ drop on a chip            → same-item tether     (members must be the same kind of item)
 *   ⌥ drop on a chip, or drop on a Σ badge → upgrade bundle (upgrades add up to one total)
 *   drop on empty board         → leave cluster / bundle
 *
 * Right-click, long-press or the ⋯ key on a chip opens the same choices as a
 * menu; "… with" then asks for the other chip, so no gesture is mouse-only.
 */

const DRAG_THRESHOLD = 5
const LONG_PRESS_MS = 480
const TETHER_COLORS = ['#58c2b4', '#c9a6e8', '#e88fb3', '#8fb7e8']

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

/** The tiny qualifiers beside a chip's name: tier, upgrade, floor. */
export function chipTags(requirement: RequirementState): string[] {
  const tags: string[] = []
  const { tier, upgrade } = requirement
  if (!requirement.item && tier.mode === 'exact') tags.push(`T${tier.value}`)
  if (!requirement.item && tier.mode === 'at_least') tags.push(`T${tier.value}+`)
  if (!requirement.item && tier.mode === 'at_most') tags.push(`T≤${tier.value}`)
  if (upgrade.mode === 'exact') tags.push(`+${upgrade.value}`)
  if (upgrade.mode === 'at_least') tags.push(`+${upgrade.value}↑`)
  if (requirement.maxDepth !== undefined) tags.push(`F≤${requirement.maxDepth}`)
  return tags
}

const tetherColor = (group: number): string => TETHER_COLORS[(group - 1) % TETHER_COLORS.length]

type DropTarget =
  | { kind: 'chip'; index: number }
  | { kind: 'cluster'; group: number }
  | { kind: 'bundle'; group: number }
  | { kind: 'board' }

interface DragState {
  source: number
  x: number
  y: number
  relation: Relation
  over: DropTarget | null
}

interface MenuState { index: number; x: number; y: number }
interface PickState { source: number; relation: Relation }

const relationFromModifiers = (event: { shiftKey: boolean; altKey: boolean }): Relation =>
  (event.shiftKey ? 'identity' : event.altKey ? 'upgradeSum' : 'alternative')

const RELATION_GLYPH: Record<Relation, string> = { alternative: 'or', identity: '=', upgradeSum: 'Σ' }
const RELATION_VERB: Record<Relation, string> = { alternative: 'Either/or with', identity: 'Same item as', upgradeSum: 'Add upgrades with' }

export function RequirementBoard({
  requirements,
  onChange,
  onEdit,
  onAdd,
}: {
  requirements: RequirementState[]
  onChange: (requirements: RequirementState[]) => void
  onEdit: (index: number) => void
  onAdd: () => void
}) {
  const boardRef = useRef<HTMLDivElement>(null)
  const [drag, setDrag] = useState<DragState | null>(null)
  const [menu, setMenu] = useState<MenuState | null>(null)
  const [pick, setPick] = useState<PickState | null>(null)
  const [hovered, setHoveredState] = useState<{ index: number; left: number; top: number } | null>(null)
  const hoveredIndex = hovered?.index ?? null
  const setHovered = (index: number | null, element?: HTMLElement) => {
    if (index === null || !element) { setHoveredState(null); return }
    const rect = element.getBoundingClientRect()
    setHoveredState({ index, left: Math.min(rect.left, window.innerWidth - 300), top: rect.bottom + 8 })
  }
  const [editingBundle, setEditingBundle] = useState<number | null>(null)
  const [tethers, setTethers] = useState<{ group: number; d: string }[]>([])
  const [boardSize, setBoardSize] = useState({ width: 0, height: 0 })
  const pressRef = useRef<{ index: number; x: number; y: number; timer: number | undefined; dragging: boolean } | null>(null)
  const dragRef = useRef<DragState | null>(null)

  const items = boardItems(requirements)

  // ---- same-item tethers: curves drawn between tethered chips -------------

  useLayoutEffect(() => {
    const board = boardRef.current
    if (!board) return
    const rect = board.getBoundingClientRect()
    const byGroup = new Map<number, { x: number; y: number }[]>()
    board.querySelectorAll<HTMLElement>('[data-chip]').forEach((element) => {
      const index = Number(element.dataset.chip)
      const group = requirements[index]?.identityGroup
      if (group === undefined) return
      const chip = element.getBoundingClientRect()
      const point = { x: chip.left + chip.width / 2 - rect.left, y: chip.bottom - rect.top }
      byGroup.set(group, [...(byGroup.get(group) ?? []), point])
    })
    const paths: { group: number; d: string }[] = []
    for (const [group, points] of byGroup) {
      for (let i = 0; i + 1 < points.length; i += 1) {
        const a = points[i]
        const b = points[i + 1]
        const sag = Math.min(18, 8 + Math.abs(b.x - a.x) / 12)
        paths.push({ group, d: `M ${a.x} ${a.y} C ${a.x} ${a.y + sag}, ${b.x} ${b.y + sag}, ${b.x} ${b.y}` })
      }
    }
    setTethers(paths)
    setBoardSize({ width: rect.width, height: rect.height })
  }, [requirements, boardSize.width])

  useEffect(() => {
    const board = boardRef.current
    if (!board || typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(([entry]) => {
      setBoardSize({ width: entry.contentRect.width, height: entry.contentRect.height })
    })
    observer.observe(board)
    return () => observer.disconnect()
  }, [])

  // ---- drag -----------------------------------------------------------------

  const targetAt = useCallback((x: number, y: number): DropTarget | null => {
    const element = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-drop]')
    if (!element || !boardRef.current?.contains(element)) return null
    const kind = element.dataset.drop
    if (kind === 'chip') return { kind: 'chip', index: Number(element.dataset.chip) }
    if (kind === 'cluster') return { kind: 'cluster', group: Number(element.dataset.group) }
    if (kind === 'bundle') return { kind: 'bundle', group: Number(element.dataset.group) }
    return { kind: 'board' }
  }, [])

  const updateDrag = (next: DragState | null) => {
    dragRef.current = next
    setDrag(next)
  }

  const firstMember = (item: BoardItem): number => (item.type === 'chip' ? item.index : item.members[0])

  const completeDrop = (state: DragState) => {
    const { source, over, relation } = state
    if (!over) return
    const current = requirements[source]
    let next: RequirementState[] | undefined
    if (over.kind === 'chip') {
      if (over.index === source) return
      next = relate(requirements, relation, source, over.index)
    } else if (over.kind === 'cluster') {
      if (current.alternativeGroup === over.group) return
      const item = items.find((entry) => entry.type === 'alternatives' && entry.group === over.group)
      if (item) next = joinAlternatives(requirements, source, firstMember(item))
    } else if (over.kind === 'bundle') {
      if (current.upgradeSum?.group === over.group) return
      const item = items.find((entry) => entry.type === 'bundle' && entry.group === over.group)
      if (item) next = joinBundle(requirements, source, firstMember(item))
    } else if (current.alternativeGroup !== undefined || current.upgradeSum) {
      next = detach(requirements, source)
    }
    if (next && next !== requirements) onChange(next)
  }

  const onChipPointerDown = (index: number) => (event: ReactPointerEvent<HTMLElement>) => {
    if (event.button !== 0) return
    if ((event.target as HTMLElement).closest('[data-chip-x]')) return
    event.currentTarget.setPointerCapture(event.pointerId)
    const timer = event.pointerType === 'mouse' ? undefined : window.setTimeout(() => {
      const press = pressRef.current
      if (!press || press.dragging) return
      pressRef.current = null
      setMenu({ index, x: press.x, y: press.y })
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
    }
    const relation = relationFromModifiers(event)
    updateDrag({ source: press.index, x: event.clientX, y: event.clientY, relation, over: targetAt(event.clientX, event.clientY) })
  }

  const onChipPointerUp = (event: ReactPointerEvent<HTMLElement>) => {
    const press = pressRef.current
    pressRef.current = null
    if (!press) return
    window.clearTimeout(press.timer)
    if (press.dragging) {
      const state = dragRef.current
      updateDrag(null)
      if (state) completeDrop({ ...state, relation: relationFromModifiers(event), over: targetAt(event.clientX, event.clientY) })
      return
    }
    // A plain tap: finish a pick, or open the editor.
    if (pick) {
      if (pick.source !== press.index) {
        const next = relate(requirements, pick.relation, pick.source, press.index)
        if (next) onChange(next)
      }
      setPick(null)
      return
    }
    onEdit(press.index)
  }

  const onChipPointerCancel = () => {
    const press = pressRef.current
    pressRef.current = null
    if (press) window.clearTimeout(press.timer)
    updateDrag(null)
  }

  // Modifier keys change the relation mid-drag without moving the pointer; Escape cancels everything.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        if (dragRef.current) { pressRef.current = null; updateDrag(null) }
        setMenu(null)
        setPick(null)
        setEditingBundle(null)
        return
      }
      const state = dragRef.current
      if (state && (event.key === 'Shift' || event.key === 'Alt')) updateDrag({ ...state, relation: relationFromModifiers(event) })
    }
    window.addEventListener('keydown', onKey)
    window.addEventListener('keyup', onKey)
    return () => {
      window.removeEventListener('keydown', onKey)
      window.removeEventListener('keyup', onKey)
    }
  }, [])

  // ---- keyboard on a chip -----------------------------------------------------

  const onChipKeyDown = (index: number) => (event: ReactKeyboardEvent<HTMLElement>) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      if (pick) {
        if (pick.source !== index) {
          const next = relate(requirements, pick.relation, pick.source, index)
          if (next) onChange(next)
        }
        setPick(null)
      } else onEdit(index)
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault()
      onChange(removeAt(requirements, index))
    } else if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10') || event.key === '.') {
      event.preventDefault()
      const rect = event.currentTarget.getBoundingClientRect()
      setMenu({ index, x: rect.left, y: rect.bottom })
    }
  }

  // ---- rendering ---------------------------------------------------------------

  const dropClass = (target: DropTarget): string => {
    if (!drag || !drag.over) return ''
    const over = drag.over
    const same = over.kind === target.kind
      && (over.kind === 'chip' ? over.index === (target as { index: number }).index
        : over.kind === 'board' ? true
          : over.group === (target as { group: number }).group)
    if (!same) return ''
    if (over.kind === 'chip') return ` d1-drop-${drag.relation}`
    if (over.kind === 'cluster') return ' d1-drop-alternative'
    if (over.kind === 'bundle') return ' d1-drop-upgradeSum'
    const source = requirements[drag.source]
    return source.alternativeGroup !== undefined || source.upgradeSum ? ' d1-drop-detach' : ''
  }

  const renderChip = (index: number) => {
    const requirement = requirements[index]
    const errors = validateRequirement(requirement)
    const tether = requirement.identityGroup
    const classes = ['d1-chip']
    if (drag?.source === index) classes.push('d1-chip-dragging')
    if (errors.length > 0) classes.push('d1-chip-error')
    if (pick) classes.push(pick.source === index ? 'd1-chip-pick-source' : 'd1-chip-pickable')
    if (hoveredIndex !== null && hoveredIndex !== index && tether !== undefined && requirements[hoveredIndex]?.identityGroup === tether) classes.push('d1-chip-peer')
    const glow = effectGlow(requirement.effect)
    const effect = effectLabel(requirement)
    const effectCount = requirement.effect !== undefined && !isAnyEnchantment(requirement.effect) ? effectNamesOf(requirement.effect, requirement.kind).length : 0
    return (
      <div
        key={index}
        role="button"
        tabIndex={0}
        className={classes.join(' ') + dropClass({ kind: 'chip', index })}
        data-drop="chip"
        data-chip={index}
        aria-label={requirementTitle(requirement)}
        style={tether !== undefined ? ({ '--d1-tether': tetherColor(tether) } as CSSProperties) : undefined}
        onPointerDown={onChipPointerDown(index)}
        onPointerMove={onChipPointerMove}
        onPointerUp={onChipPointerUp}
        onPointerCancel={onChipPointerCancel}
        onKeyDown={onChipKeyDown(index)}
        onContextMenu={(event) => {
          event.preventDefault()
          setMenu({ index, x: event.clientX, y: event.clientY })
        }}
        onMouseEnter={(event) => setHovered(index, event.currentTarget)}
        onMouseLeave={() => { if (hoveredIndex === index) setHovered(null) }}
        onFocus={(event) => setHovered(index, event.currentTarget)}
        onBlur={() => { if (hoveredIndex === index) setHovered(null) }}
      >
        <Sprite index={requirementSprite(requirement)} size={18} glow={glow} />
        <span className="d1-chip-name">{chipName(requirement)}</span>
        {chipTags(requirement).map((tag) => <span key={tag} className="d1-chip-tag">{tag}</span>)}
        {effect && (
          <span
            className={`d1-chip-effect${isAnyEnchantment(requirement.effect) ? ' d1-chip-effect-any' : glow ? '' : ' d1-chip-effect-curse'}`}
            style={glow ? { background: glow.color } : undefined}
            title={effect}
          >
            {effectCount > 1 ? effectCount : ''}
          </span>
        )}
        {requirement.uncursed && <span className="d1-chip-tag d1-chip-tag-soft" title="Uncursed">✓</span>}
        {tether !== undefined && <span className="d1-chip-tether" aria-hidden="true" />}
        <button
          type="button"
          className="d1-chip-x"
          data-chip-x
          tabIndex={-1}
          aria-label="Remove requirement"
          title="Remove"
          onClick={(event) => {
            event.stopPropagation()
            onChange(removeAt(requirements, index))
          }}
        >
          <XIcon size={11} />
        </button>
      </div>
    )
  }

  const renderItem = (item: BoardItem): ReactNode => {
    if (item.type === 'chip') return renderChip(item.index)
    if (item.type === 'alternatives') {
      return (
        <div key={`alt:${item.group}`} className={`d1-cluster${dropClass({ kind: 'cluster', group: item.group })}`} data-drop="cluster" data-group={item.group} role="group" aria-label={`Any of ${item.members.length}`}>
          {item.members.map((index, position) => (
            <span key={index} className="d1-cluster-member">
              {position > 0 && <span className="d1-cluster-or" aria-hidden="true">or</span>}
              {renderChip(index)}
            </span>
          ))}
        </div>
      )
    }
    const members = item.members.map((index) => requirements[index])
    const capacity = upgradeSumCapacity(members)
    return (
      <div key={`sum:${item.group}`} className={`d1-bundle${item.atLeast > capacity ? ' d1-bundle-error' : ''}${dropClass({ kind: 'bundle', group: item.group })}`} data-drop="bundle" data-group={item.group} role="group" aria-label={`Upgrades add to at least +${item.atLeast}`}>
        {item.members.map((index) => renderChip(index))}
        {editingBundle === item.group ? (
          <span className="d1-bundle-edit" role="group" aria-label="Combined upgrade total">
            <button type="button" aria-label="Lower total" disabled={item.atLeast <= 1} onClick={() => onChange(setBundleTotal(requirements, item.group, item.atLeast - 1))}>−</button>
            <span className="d1-bundle-badge">Σ ≥ +{item.atLeast}</span>
            <button type="button" aria-label="Raise total" disabled={item.atLeast >= capacity} onClick={() => onChange(setBundleTotal(requirements, item.group, item.atLeast + 1))}>+</button>
            <button type="button" className="d1-bundle-done" aria-label="Done" onClick={() => setEditingBundle(null)}>✓</button>
          </span>
        ) : (
          <button
            type="button"
            className="d1-bundle-badge d1-bundle-badge-btn"
            title={`Upgrades add to at least +${item.atLeast} (up to +${capacity} possible)`}
            onClick={() => setEditingBundle(item.group)}
          >
            Σ ≥ +{item.atLeast}
          </button>
        )}
      </div>
    )
  }

  const dragSource = drag ? requirements[drag.source] : null
  const statusLine = pick
    ? `${RELATION_VERB[pick.relation]}… choose a chip`
    : drag ? 'drop on a chip · ⇧ same item · ⌥ add upgrades · empty space detaches' : null

  return (
    <div className="d1-board-wrap">
      <div
        ref={boardRef}
        className={`d1-board${drag ? ' d1-board-dragging' : ''}${dropClass({ kind: 'board' })}`}
        data-drop="board"
        onMouseDown={() => setMenu(null)}
      >
        <svg className="d1-tethers" width={boardSize.width} height={boardSize.height + 20} aria-hidden="true">
          {tethers.map((tether, i) => (
            <path key={i} d={tether.d} stroke={tetherColor(tether.group)} />
          ))}
        </svg>
        {items.map(renderItem)}
        <button type="button" className="d1-chip d1-chip-add" onClick={onAdd} title="Add a requirement" aria-label="Add a requirement">
          <PlusIcon size={13} />
        </button>
      </div>
      {statusLine && <div className="d1-board-status d1-mono" aria-live="polite">{statusLine}</div>}
      {hovered && !drag && !menu && requirements[hovered.index] && (
        <ChipPopover
          requirement={requirements[hovered.index]}
          requirements={requirements}
          errors={validateRequirement(requirements[hovered.index])}
          style={{ left: hovered.left, top: hovered.top }}
        />
      )}
      {drag && dragSource && (
        <div className="d1-chip d1-chip-ghost" style={{ left: drag.x, top: drag.y }} aria-hidden="true">
          <Sprite index={requirementSprite(dragSource)} size={18} />
          <span className="d1-chip-name">{chipName(dragSource)}</span>
          {drag.over && drag.over.kind !== 'board' && (
            <span className={`d1-chip-ghost-tag d1-ghost-${drag.over.kind === 'chip' ? drag.relation : drag.over.kind === 'cluster' ? 'alternative' : 'upgradeSum'}`}>
              {RELATION_GLYPH[drag.over.kind === 'chip' ? drag.relation : drag.over.kind === 'cluster' ? 'alternative' : 'upgradeSum']}
            </span>
          )}
        </div>
      )}
      {menu && (
        <ChipMenu
          state={menu}
          requirement={requirements[menu.index]}
          canPick={requirements.length > 1}
          onClose={() => setMenu(null)}
          onEdit={() => { setMenu(null); onEdit(menu.index) }}
          onPick={(relation) => { setMenu(null); setPick({ source: menu.index, relation }) }}
          onDetach={() => { setMenu(null); onChange(detach(requirements, menu.index)) }}
          onUnlink={() => { setMenu(null); onChange(unlinkIdentity(requirements, menu.index)) }}
          onRemove={() => { setMenu(null); onChange(removeAt(requirements, menu.index)) }}
        />
      )}
    </div>
  )
}

/** The detail card under a hovered or focused chip. */
function ChipPopover({ requirement, requirements, errors, style }: { requirement: RequirementState; requirements: RequirementState[]; errors: string[]; style: CSSProperties }) {
  const lines: string[] = []
  if (requirement.upgrade.mode === 'exact') lines.push(`exactly +${requirement.upgrade.value}`)
  else if (requirement.upgrade.mode === 'at_least') lines.push(`+${requirement.upgrade.value} or higher`)
  else lines.push('any upgrade')
  const effect = effectLabel(requirement)
  if (effect) lines.push(effect)
  if (requirement.uncursed) lines.push('uncursed')
  if (requirement.source) lines.push(sourceLabel(requirement.source))
  if (requirement.maxDepth !== undefined) lines.push(`floors 1–${requirement.maxDepth}`)
  const peers = (match: (other: RequirementState) => boolean) =>
    requirements.filter((other) => other !== requirement && match(other)).map(chipName)
  const relations: { glyph: string; text: string; color?: string }[] = []
  if (requirement.alternativeGroup !== undefined) {
    relations.push({ glyph: 'or', text: peers((other) => other.alternativeGroup === requirement.alternativeGroup).join(', ') })
  }
  if (requirement.identityGroup !== undefined) {
    relations.push({ glyph: '=', text: `same item as ${peers((other) => other.identityGroup === requirement.identityGroup).join(', ')}`, color: tetherColor(requirement.identityGroup) })
  }
  if (requirement.upgradeSum) {
    const sum = requirement.upgradeSum
    relations.push({ glyph: 'Σ', text: `≥ +${sum.atLeast} with ${peers((other) => other.upgradeSum?.group === sum.group).join(', ')}` })
  }
  return (
    <div className="d1-chip-pop" role="tooltip" style={style}>
      <div className="d1-chip-pop-title">{requirementTitle(requirement)}</div>
      <div className="d1-chip-pop-sub">{lines.join(' · ')}</div>
      {relations.map((relation) => (
        <div key={relation.glyph} className="d1-chip-pop-rel" style={relation.color ? { color: relation.color } : undefined}>
          <span className="d1-chip-pop-glyph">{relation.glyph}</span>
          <span>{relation.text}</span>
        </div>
      ))}
      {errors.length > 0 && <div className="d1-chip-pop-error">{errors[0]}</div>}
      {requirementKind(requirement) === undefined && <div className="d1-chip-pop-error">No category</div>}
    </div>
  )
}

/** The chip's context menu: the drag gestures as words, for keyboard and touch. */
function ChipMenu({
  state,
  requirement,
  canPick,
  onClose,
  onEdit,
  onPick,
  onDetach,
  onUnlink,
  onRemove,
}: {
  state: MenuState
  requirement: RequirementState
  canPick: boolean
  onClose: () => void
  onEdit: () => void
  onPick: (relation: Relation) => void
  onDetach: () => void
  onUnlink: () => void
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
  const left = Math.min(state.x, window.innerWidth - 220)
  const top = Math.min(state.y, window.innerHeight - 240)
  const grouped = requirement.alternativeGroup !== undefined || requirement.upgradeSum !== undefined
  return (
    <div ref={ref} className="d1-chip-menu" role="menu" style={{ left, top }}>
      <button type="button" role="menuitem" onClick={onEdit}>Edit…</button>
      {canPick && (
        <>
          <span className="d1-chip-menu-rule" />
          <button type="button" role="menuitem" onClick={() => onPick('alternative')}><b>or</b>{RELATION_VERB.alternative}…</button>
          <button type="button" role="menuitem" onClick={() => onPick('identity')}><b>=</b>{RELATION_VERB.identity}…</button>
          {requirement.alternativeGroup === undefined && (
            <button type="button" role="menuitem" onClick={() => onPick('upgradeSum')}><b>Σ</b>{RELATION_VERB.upgradeSum}…</button>
          )}
        </>
      )}
      {(grouped || requirement.identityGroup !== undefined) && <span className="d1-chip-menu-rule" />}
      {grouped && <button type="button" role="menuitem" onClick={onDetach}>Detach from group</button>}
      {requirement.identityGroup !== undefined && <button type="button" role="menuitem" onClick={onUnlink}>Cut same-item link</button>}
      <span className="d1-chip-menu-rule" />
      <button type="button" role="menuitem" className="d1-chip-menu-danger" onClick={onRemove}>Remove</button>
    </div>
  )
}

import { useEffect, type ComponentPropsWithRef } from 'react'
import type { FollowUpSuggestion } from './sessionSuggestions'

type Props = ComponentPropsWithRef<'textarea'> & {
  suggestion: FollowUpSuggestion | null
  dismissSuggestion: () => void
  onAcceptSuggestion: (text: string) => void
}

/** Both desktop composers use their existing placeholder style for the ghost text. */
export function SuggestionTextarea({ suggestion, dismissSuggestion, onAcceptSuggestion, ...props }: Props) {
  const ghost = !props.value && !props.disabled ? suggestion?.text : undefined

  useEffect(() => {
    if (props.value && suggestion) dismissSuggestion()
  }, [props.value, suggestion, dismissSuggestion])

  return <textarea
    {...props}
    placeholder={ghost ?? props.placeholder}
    aria-description={ghost ? `Suggested next message: ${ghost} Press Tab or Right Arrow to insert, or Escape to dismiss.` : props['aria-description']}
    onChange={(event) => {
      if (event.currentTarget.value) dismissSuggestion()
      props.onChange?.(event)
    }}
    onKeyDown={(event) => {
      const { key, currentTarget } = event
      if (ghost && !event.nativeEvent.isComposing && !event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey) {
        const atEnd = currentTarget.selectionStart === currentTarget.value.length && currentTarget.selectionEnd === currentTarget.value.length
        const accept = key === 'Tab' || (key === 'ArrowRight' && atEnd)
        if (accept || key === 'Escape') {
          event.preventDefault()
          event.stopPropagation()
          dismissSuggestion()
          if (accept) onAcceptSuggestion(ghost)
          return
        }
      }
      props.onKeyDown?.(event)
    }}
  />
}

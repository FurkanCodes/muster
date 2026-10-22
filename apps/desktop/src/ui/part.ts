// How the design system's own parts are made, as Base UI makes its own
// (its useRender and mergeProps): each draws one element with its classes,
// the caller's props merged over its own (classes joined, handlers both
// called, the caller's last), and a `render` that puts another element or
// a component in its place, as every Base UI part takes.
//
//   export function Label(props: PartProps<"span">) {
//     return usePart("span", props, { className: "mu-label" });
//   }

import { mergeProps } from "@base-ui/react/merge-props";
import { useRender } from "@base-ui/react/use-render";
import { createElement, type JSX, type HTMLProps as ReactHTMLProps } from "react";

type Tag = keyof JSX.IntrinsicElements;

/** A part's props: its element's own, and `render`. */
export type PartProps<T extends Tag> = useRender.ComponentProps<T>;

/** `tag` drawn with `own` props and the caller's `props` over them. */
export function usePart<T extends Tag>(
  tag: T,
  { render, ...props }: PartProps<T>,
  own: useRender.ElementProps<T>,
) {
  // Merged as an element's props whatever the tag: the types are a div's,
  // the merging the same for any element.
  type Props = useRender.ElementProps<"div">;
  return useRender({
    defaultTagName: tag,
    render,
    props: mergeProps<"div">(own as Props, props as Props),
  });
}

/**
 * A Base UI button drawn as the browser's own: without the tabindex of 0
 * Base UI gives every button, so in WebKit (the app's window) a click
 * leaves the focus where it was and ⇥ reaches it only where the person's
 * settings say, as a plain button does. Any other tabindex (-1, a roving
 * group's) stays. For a part's `render`.
 */
export function nativeButton(props: ReactHTMLProps<HTMLButtonElement>) {
  const { tabIndex, ...rest } = props;
  return createElement("button", {
    ...rest,
    type: (rest.type as "button" | "submit" | "reset" | undefined) ?? "button",
    tabIndex: tabIndex === 0 ? undefined : tabIndex,
  });
}

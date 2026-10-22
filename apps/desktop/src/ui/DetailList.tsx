// Facts as name and value, two columns (design system: DetailList).
//
//   <DetailList>
//     <Detail name="From">github.com/…</Detail>
//   </DetailList>

import type { ReactNode } from "react";
import { type PartProps, usePart } from "./part";

export function DetailList(props: PartProps<"dl">) {
  return usePart("dl", props, { className: "mu-details" });
}

/** One fact: its name, then its value (the part, its `render` the value's). */
export function Detail({ name, ...props }: PartProps<"dd"> & { name: ReactNode }) {
  return (
    <>
      <dt>{name}</dt>
      {usePart("dd", props, {})}
    </>
  );
}

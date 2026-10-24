// Everything a plugin runs and needs, as name and value (design system:
// DetailList): each command word for word, each program found or not, the
// variables and secrets it is given and its whole manifest hash, never
// cut short, for its details and before it is approved.

import type { ReactNode } from "react";
import type { PluginDetails } from "../../ipc/generated/PluginDetails";
import { oneLine } from "../../lib/text";
import { Detail, DetailList } from "../../ui/DetailList";
import { Found, Missing } from "../../ui/status";
import { missingPrograms, runsOf } from "./pluginWords";

/** Its variables and secrets, by name: `YOUTRACK_SITE, and the secret YOUTRACK_TOKEN`. */
function given(details: PluginDetails): ReactNode {
  const env = details.env.map((name) => <code key={`env-${name}`}>{oneLine(name)}</code>);
  const secrets = details.secrets.map((name) => (
    <code key={`secret-${name}`}>{oneLine(name)}</code>
  ));
  const parts: ReactNode[] = [];
  env.forEach((item, at) => {
    if (at > 0) parts.push(", ");
    parts.push(item);
  });
  if (secrets.length > 0) {
    if (env.length > 0) parts.push(", and ");
    parts.push(secrets.length === 1 ? "the secret " : "the secrets ");
    secrets.forEach((item, at) => {
      if (at > 0) parts.push(", ");
      parts.push(item);
    });
  }
  return parts;
}

export function PluginFacts({
  details,
  from,
  children,
}: {
  details: PluginDetails;
  /** Where it is from, first. */
  from: ReactNode;
  /** More facts, last: its last change, what it is now. */
  children?: ReactNode;
}) {
  const missing = missingPrograms(details);
  return (
    <DetailList>
      <Detail name="From">{from}</Detail>
      <Detail name="Version">
        <code>{oneLine(details.version)}</code>, for Muster <code>{oneLine(details.muster)}</code>{" "}
        or later
      </Detail>
      {runsOf(details).map((runs) => (
        <RunsFact key={`${runs.what}-${runs.command}`} what={runs.what}>
          <code>{runs.command}</code>, {runs.how}
        </RunsFact>
      ))}
      {details.requires.length > 0 ? (
        <Detail name="Needs">
          {details.requires.map((required, at) => (
            <span key={required.program}>
              {at > 0 ? <br /> : null}
              <code>{oneLine(required.program)}</code>{" "}
              {required.found ? <Found>✓ found</Found> : <Missing>✗ not found</Missing>}
            </span>
          ))}
          {missing.length > 0 && details.how_to_get ? (
            <>
              <br />
              <span className="mu-quiet">{oneLine(details.how_to_get)}</span>
            </>
          ) : null}
        </Detail>
      ) : null}
      {details.env.length > 0 || details.secrets.length > 0 ? (
        <Detail name="Is given">{given(details)}</Detail>
      ) : null}
      <Detail name="Manifest">
        <code>sha256 {details.sha256}</code>
      </Detail>
      {children}
    </DetailList>
  );
}

function RunsFact({ what, children }: { what: string; children: ReactNode }) {
  return <Detail name={what}>{children}</Detail>;
}

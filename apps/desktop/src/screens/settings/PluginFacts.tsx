// Everything a plugin runs and needs, as name and value (design system:
// DetailList): each command word for word, each program found or not, the
// variables and secrets it is given and its whole manifest hash, never
// cut short, for its details and before it is approved.

import type { ReactNode } from "react";
import type { PluginDetails } from "../../ipc/generated/PluginDetails";
import { missingPrograms, runsOf } from "./pluginWords";
import { oneLine } from "./words";

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
    <dl className="mu-details">
      <dt>From</dt>
      <dd>{from}</dd>
      <dt>Version</dt>
      <dd>
        <code>{oneLine(details.version)}</code>, for Muster <code>{oneLine(details.muster)}</code>{" "}
        or later
      </dd>
      {runsOf(details).map((runs) => (
        <RunsFact key={`${runs.what}-${runs.command}`} what={runs.what}>
          <code>{runs.command}</code>, {runs.how}
        </RunsFact>
      ))}
      {details.requires.length > 0 ? (
        <>
          <dt>Needs</dt>
          <dd>
            {details.requires.map((required, at) => (
              <span key={required.program}>
                {at > 0 ? <br /> : null}
                <code>{oneLine(required.program)}</code>{" "}
                {required.found ? (
                  <span className="mu-found">✓ found</span>
                ) : (
                  <span className="mu-missing">✗ not found</span>
                )}
              </span>
            ))}
            {missing.length > 0 && details.how_to_get ? (
              <>
                <br />
                <span className="mu-quiet">{oneLine(details.how_to_get)}</span>
              </>
            ) : null}
          </dd>
        </>
      ) : null}
      {details.env.length > 0 || details.secrets.length > 0 ? (
        <>
          <dt>Is given</dt>
          <dd>{given(details)}</dd>
        </>
      ) : null}
      <dt>Manifest</dt>
      <dd>
        <code>sha256 {details.sha256}</code>
      </dd>
      {children}
    </dl>
  );
}

function RunsFact({ what, children }: { what: string; children: ReactNode }) {
  return (
    <>
      <dt>{what}</dt>
      <dd>{children}</dd>
    </>
  );
}

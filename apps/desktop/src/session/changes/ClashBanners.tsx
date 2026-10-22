// Where this workspace's work meets other work in its project, as the TUI
// says it (view::clashes): a banner for each other workspace, a conflict
// in `waiting`, an overlap in `working`, each with a key to see that
// workspace's changes.

import { useActions, useApp, useDaemon } from "../../app/context";
import { useKeys } from "../../app/keys";
import type { Clash } from "../../ipc/generated/Clash";
import { standsBeside } from "../../state/selectors";
import { Banner, BannerLead, BannerText } from "../../ui/Banner";
import { Button } from "../../ui/controls";
import { clashFiles, clashWords, placeName, wouldConflict } from "./labels";

function ClashBanner({ clash, index }: { clash: Clash; index: number }) {
  const place = useDaemon((state) => placeName(state.workspaces[clash.with]));
  const app = useApp();
  const actions = useActions();
  const conflict = wouldConflict(clash);
  const key = String(index + 1);
  const { named, more } = clashFiles(clash);
  const lead = conflict ? "Conflict" : "Overlap";
  const open = () => {
    const session = Object.values(app.daemon.store.getState().sessions).find(
      (session) => session.workspace === clash.with && !standsBeside(session),
    );
    if (!session) {
      actions.tell(`Nothing runs in ${place}, so it has no pane to show its changes in.`, true);
      return;
    }
    actions.select("all", session.id);
    actions.show("all");
    actions.showPane("changes");
  };
  useKeys({ [key]: open });
  return (
    <Banner tone={conflict ? "waiting" : "overlap"}>
      <BannerText title={`${lead} with ${place}: ${clashWords(clash)}`}>
        <BannerLead>{lead}</BannerLead> with {place}:{" "}
        {conflict ? "would conflict in " : "both change "}
        <span className="mu-mono">{named}</span>
        {more ? ` and ${more} more` : ""}
        {conflict ? "." : ". Git could merge them."}
      </BannerText>
      <Button size="sm" title={`See its changes (${key})`} onClick={open}>
        See its changes
      </Button>
    </Banner>
  );
}

export function ClashBanners({ workspace }: { workspace: string }) {
  const clashes = useDaemon((state) => state.workspaces[workspace]?.clashes);
  if (!clashes?.length) return null;
  return (
    <div className="changes-clashes">
      {clashes.map((clash, index) => (
        <ClashBanner key={clash.with} clash={clash} index={index} />
      ))}
    </div>
  );
}

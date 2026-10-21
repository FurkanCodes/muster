import { useActions, useApp, useDaemon } from "../../app/context";
import { useKeys } from "../../app/keys";
import type { Clash } from "../../ipc/generated/Clash";
import { branchOf } from "../../state/selectors";
import { Button } from "../../ui/controls";
import { clashWords } from "./labels";

function ClashBanner({ clash, index }: { clash: Clash; index: number }) {
  const other = useDaemon((state) => state.workspaces[clash.with]);
  const app = useApp();
  const actions = useActions();
  const open = () => {
    const session = Object.values(app.daemon.store.getState().sessions).find(
      (session) => session.workspace === clash.with && session.harness !== "shell",
    );
    if (!session) {
      actions.tell("No session in that workspace.", true);
      return;
    }
    actions.select("all", session.id);
    actions.show("all");
    actions.showPane("changes");
  };
  useKeys({ [String(index + 1)]: open });
  return (
    <div className={`mu-banner ${clash.conflicts ? "" : "mu-banner--overlap"}`} role="status">
      <span className="mu-banner-text changes-clash-text">
        <span className="mu-banner-lead">{clash.conflicts ? "Conflict" : "Overlap"}</span>
        {" · "}
        <span className="mu-branch">{other ? branchOf(other) : "another worktree"}</span>{" "}
        {clashWords(clash)}
      </span>
      <Button size="sm" title={`See the other workspace (${index + 1})`} onClick={open}>
        See workspace
      </Button>
    </div>
  );
}

export function ClashBanners({ workspace }: { workspace: string }) {
  const clashes = useDaemon((state) => state.workspaces[workspace]?.clashes);
  return (
    <>
      {clashes?.map((clash, index) => (
        <ClashBanner key={clash.with} clash={clash} index={index} />
      ))}
    </>
  );
}

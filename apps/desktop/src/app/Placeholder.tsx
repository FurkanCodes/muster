import { KepPortrait } from "../ui/Kep";

/** Stands in for a screen until its branch lands. */
export function Placeholder({ title, branch }: { title: string; branch: string }) {
  return (
    <div className="app-placeholder">
      <div className="mu-empty">
        <KepPortrait />
        <h1 className="mu-empty-title">{title}</h1>
        <p className="mu-empty-text">
          Being built on <span className="mu-mono">{branch}</span>.
        </p>
      </div>
    </div>
  );
}

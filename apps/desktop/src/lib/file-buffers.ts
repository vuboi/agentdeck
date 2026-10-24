// Unsaved edits of open file tabs, by tab id. Only the active tab's viewer is mounted, so a tab that is not in
// front keeps its edits here until they are saved or discarded.
export type Draft = {
  /** The file as it was last read or written (raw: BOM and line ends included). */
  disk: string;
  /** The edited text, as held while the viewer is not mounted. */
  text: string;
  /** While the viewer is mounted: the text in the editor right now. */
  read?: () => string;
  /** While the viewer is mounted: told after a save from outside (tab menu) with what was written. */
  saved?: (raw: string) => void;
};

export const drafts = new Map<string, Draft>();

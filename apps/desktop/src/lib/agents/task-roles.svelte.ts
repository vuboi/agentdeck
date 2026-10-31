// The saved task roles (Settings > Task roles): each role's ordered routes, the master flag and the roles switched off.
// Kept in the prefs file like the other settings. Switching a role off keeps its picks, so switching it on shows them again.
import { getPref, setPref } from '../app/platform/prefs';
import { migrateOff, migrateRoutes, type RoleState } from './task-roles';

export const taskRoles = $state<RoleState>({ routes: {}, on: true, off: [] });

/** The part of the Tasks menu that is open: kept here so it is still open after a visit to another menu. */
export const tasksTab = $state({ value: 'fallback' });

/** A role's body is open while it is not switched off and it has a row (an empty one, when it was just switched on). */
export const roleOpen = (id: string) => !taskRoles.off.includes(id) && (taskRoles.routes[id]?.length ?? 0) > 0;

/** Switches a role on (it gets one empty row to pick into when it has none) or off (its picks are kept, unused). */
export function setRoleOpen(id: string, on: boolean) {
  taskRoles.off = on ? taskRoles.off.filter((item) => item !== id) : [...taskRoles.off.filter((item) => item !== id), id];
  if (on) {
    if (!taskRoles.routes[id]?.length) taskRoles.routes[id] = [''];
  } else {
    // The empty row goes with the switch-off; the picks stay.
    taskRoles.routes[id] = (taskRoles.routes[id] ?? []).filter((route) => route !== '');
  }
}

const KEY = 'task.roles';
/** Set once the picks saved before the roles were cleared (the test picks) have been dropped. */
const CLEARED_KEY = 'task.roles.cleared';
let hydrated = false;
void Promise.all([getPref<Partial<RoleState> | Record<string, unknown>>(KEY), getPref<boolean>(CLEARED_KEY)]).then(([saved, cleared]) => {
  if (!cleared) {
    void setPref(CLEARED_KEY, true);
    void setPref(KEY, { routes: {}, on: true, off: [] });
  } else if (saved && 'routes' in saved) {
    // Saved as `{ routes, on, off }`, or as `{ routes, on }` / `{ routes, off }` by the versions before.
    const current = saved as Partial<RoleState>;
    const routes = current.routes ?? {};
    taskRoles.routes = migrateRoutes(routes);
    taskRoles.on = current.on ?? true;
    taskRoles.off = migrateOff(current.off ?? [], routes);
  } else if (saved) {
    taskRoles.routes = migrateRoutes(saved as RoleState['routes']);
  }
  hydrated = true;
});

$effect.root(() => {
  $effect(() => {
    // A row with no model is not kept on disk: it is only there until the page reloads.
    const snapshot = $state.snapshot(taskRoles);
    const routes = Object.fromEntries(Object.entries(snapshot.routes).map(([id, list]) => [id, (list ?? []).filter((route) => route !== '')]));
    if (hydrated) setPref(KEY, { routes, on: snapshot.on, off: snapshot.off });
  });
});

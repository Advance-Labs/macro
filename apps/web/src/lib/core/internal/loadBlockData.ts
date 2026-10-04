/**
 * Load block data and its lazy component in parallel. Keep publication ordered
 * after component evaluation while the remaining legacy hosts use this loader.
 */
export async function loadBlockDataAfterComponentPreload<T>(
  load: () => Promise<T>,
  preload?: () => Promise<unknown>
): Promise<T> {
  const componentPromise = preload?.() ?? Promise.resolve();
  const dataPromise = load();
  const [result] = await Promise.all([dataPromise, componentPromise]);
  return result;
}

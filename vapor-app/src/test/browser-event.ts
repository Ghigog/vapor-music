/**
 * Event listening, for the browser-served fake.
 *
 * The fake pushes nothing, so listeners are registered and never fire. That is
 * the correct quiescent behaviour: the app polls where it must and listens
 * where the backend would push, and neither should hang waiting for an event
 * that is not coming.
 */
export async function listen(): Promise<() => void> {
  return () => {};
}

export async function emit(): Promise<void> {}

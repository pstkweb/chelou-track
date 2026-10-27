/**
 * File d'exécution strictement séquentielle : chaque tâche démarre quand la précédente est
 * terminée (succès ou échec). Renvoie le résultat de la tâche mise en file.
 */
export function createSerialQueue() {
  let tail: Promise<unknown> = Promise.resolve();

  return <T>(task: () => Promise<T>): Promise<T> => {
    const run = tail.then(task, task);
    tail = run.catch(() => {});
    return run;
  };
}

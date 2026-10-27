import { describe, expect, it } from 'vitest';
import { createSerialQueue } from './serial-queue';

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('createSerialQueue', () => {
  it('starts a task only after the previous one has settled', async () => {
    const enqueue = createSerialQueue();
    const log: string[] = [];
    let releaseFirst: () => void = () => {};

    enqueue(
      () =>
        new Promise<void>((resolve) => {
          log.push('first:start');
          releaseFirst = () => {
            log.push('first:end');
            resolve();
          };
        }),
    );
    const second = enqueue(async () => {
      log.push('second');
    });

    await tick();
    expect(log).toEqual(['first:start']);
    releaseFirst();
    await second;
    expect(log).toEqual(['first:start', 'first:end', 'second']);
  });

  it('keeps going after a failed task and reports its own result', async () => {
    const enqueue = createSerialQueue();
    const failed = enqueue(async () => {
      throw new Error('boom');
    });
    const next = enqueue(async () => 42);

    await expect(failed).rejects.toThrow('boom');
    await expect(next).resolves.toBe(42);
  });
});

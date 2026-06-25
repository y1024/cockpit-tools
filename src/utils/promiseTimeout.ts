export function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number,
  timeoutError: string | (() => Error),
): Promise<T> {
  if (timeoutMs <= 0) return promise;

  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => {
      reject(
        typeof timeoutError === "function"
          ? timeoutError()
          : new Error(timeoutError),
      );
    }, timeoutMs);

    promise.then(
      (value) => {
        window.clearTimeout(timer);
        resolve(value);
      },
      (error) => {
        window.clearTimeout(timer);
        reject(error);
      },
    );
  });
}

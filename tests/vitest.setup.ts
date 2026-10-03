import {
  beforeAll,
  afterAll,
  beforeEach,
  afterEach,
  it as originalIt,
  test as originalTest,
} from "vitest";

// Mocha-to-Vitest compatibility bridge for legacy Anchor test suites
(globalThis as any).before = function (fn: any) {
  return beforeAll(async function (this: any) {
    const scope = this ?? {};
    if (typeof scope.timeout !== "function") {
      scope.timeout = (_ms: number) => {};
    }
    return fn.apply(scope, arguments);
  });
};

(globalThis as any).after = function (fn: any) {
  return afterAll(async function (this: any) {
    const scope = this ?? {};
    if (typeof scope.timeout !== "function") {
      scope.timeout = (_ms: number) => {};
    }
    return fn.apply(scope, arguments);
  });
};

(globalThis as any).beforeEach = beforeEach;
(globalThis as any).afterEach = afterEach;

const wrapTest = (target: any) => {
  if (typeof target !== "function") return target;
  const wrapped = function (name: string, fn?: any, timeout?: any) {
    if (typeof fn !== "function") return target(name, fn, timeout);
    return target(
      name,
      function (this: any) {
        const scope = this ?? {};
        if (typeof scope.timeout !== "function") {
          scope.timeout = (_ms: number) => {};
        }
        return fn.apply(scope, arguments);
      },
      timeout
    );
  };
  Object.assign(wrapped, target);
  for (const method of ["only", "skip", "todo", "fails", "concurrent"]) {
    if (typeof (target as any)[method] === "function") {
      (wrapped as any)[method] = function (
        name: string,
        fn?: any,
        timeout?: any
      ) {
        if (typeof fn !== "function")
          return (target as any)[method](name, fn, timeout);
        return (target as any)[method](
          name,
          function (this: any) {
            const scope = this ?? {};
            if (typeof scope.timeout !== "function") {
              scope.timeout = (_ms: number) => {};
            }
            return fn.apply(scope, arguments);
          },
          timeout
        );
      };
    }
  }
  return wrapped;
};

(globalThis as any).it = wrapTest(originalIt);
(globalThis as any).test = wrapTest(originalTest);

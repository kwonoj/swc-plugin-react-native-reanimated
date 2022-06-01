import * as path from "path";

const options = {
  filename: path.join(path.resolve(__dirname, ".."), "jest tests fixture"),
  jsc: {
    parser: {
      syntax: "ecmascript",
      jsx: true,
    },
    target: "es2022",
    preserveAllComments: true,
    experimental: {},
  },
  isModule: true,
  module: {
    type: "commonjs",
  },
};

const transformPresets: Array<
  [
    string,
    (code: string) => ReturnType<typeof import("@swc/core").transformSync>
  ]
> = [
  /*
    ['plugin', (code: string) => {
      const opt = { ...options };
      opt.jsc.experimental = {
        plugins: [
          [
            path.resolve(
              __dirname,
              "../target/wasm32-wasi/debug/swc_plugin_reanimated.wasm"
            ),
            {},
          ],
        ],
      }

      const { transformSync } = require('@swc/core');
      return transformSync(code, opt)
    }],*/
  [
    "custom transform",
    (code: string) => {
      const { transformSync } = require("../index");
      return transformSync(code, true, Buffer.from(JSON.stringify(options)));
    },
  ],
];

describe.each(transformPresets)("fixture with %s", (_, executeTransform) => {
  it("workletizes object hook wrapped ObjectMethod automatically", () => {
    const input = `
      useAnimatedGestureHandler({
        onStart(event) {
          console.log(event);
        },
      });
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).toMatchInlineSnapshot(`
      "\\"use strict\\";
      useAnimatedGestureHandler({
          onStart: function() {
              const _f = function _f(event) {
                  console.log(event);
              };
              _f._closure = {};
              _f.asString = \\"function onStart(event){console.log(event);}\\";
              _f.__workletHash = 4276664511;
              _f.__location = \\"${process.cwd()}/jest tests fixture (3:8)\\";
              return _f;
          }
      });
      "
    `);
  });
});

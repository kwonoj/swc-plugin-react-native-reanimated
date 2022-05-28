import * as path from "path";

const options = {
  filename: 'jest fixtures',
  jsc: {
    parser: {
      syntax: "ecmascript",
      jsx: true,
    },
    target: "es2022",
    preserveAllComments: true,
    experimental: {}
  },
  isModule: true,
  module: {
    type: "commonjs"
  },
}

const transformPresets: Array<[string, (code: string) => ReturnType<typeof import('@swc/core').transformSync>]> = [
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
  }],
  ['custom transform', (code: string) => {
    const { transformSync } = require("../index");
    return transformSync(
      code,
      true,
      Buffer.from(JSON.stringify(options))
    );
  }]
];

describe.each(transformPresets)('fixture with %s', (_, executeTransform) => {
  it('transforms', () => {
    const input = `
    import Animated, {
      useAnimatedStyle,
      useSharedValue,
    } from 'react-native-reanimated';

    function Box() {
      const offset = useSharedValue(0);

      const animatedStyles = useAnimatedStyle(() => {
        return {
          transform: [{ translateX: offset.value * 255 }],
        };
      });

      return (
        <>
          <Animated.View style={[styles.box, animatedStyles]} />
          <Button onPress={() => (offset.value = Math.random())} title="Move" />
        </>
      );
    }
  `;

    const { code } = executeTransform(input);
    expect(code).toMatchSnapshot();
  });
});
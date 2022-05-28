const transformPresets: Array<[string, Function]> = [
  ['plugin', () => { console.log('c'); }],
  ['custom transform', () => { console.log('p'); }]
]

describe.each(transformPresets)('fixture with %s', (_, transform) => {
  it('dummy', () => {
    transform();
    expect(true).toBe(true);
  });
});
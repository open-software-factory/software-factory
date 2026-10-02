// Fail the build on Web Storage or a leftover require() in a page; the design tracker's guards table says why.
const STORAGE = /\b(localStorage|sessionStorage)\b/;
const REQUIRE = /\brequire\(/;

function isExempt(fileName) {
  return fileName.startsWith('spikes/');
}

export default function outputGuards() {
  return {
    name: 'console-lab:output-guards',
    generateBundle(_options, bundle) {
      const storageHits = [];
      const requireHits = [];
      for (const [fileName, output] of Object.entries(bundle)) {
        if (isExempt(fileName)) continue;
        const code = output.type === 'chunk' ? output.code
          : typeof output.source === 'string' ? output.source : null;
        if (code == null) continue;
        if (STORAGE.test(code)) storageHits.push(fileName);
        if (REQUIRE.test(code)) requireHits.push(fileName);
      }
      if (storageHits.length) {
        this.error(
          'Web Storage reference in the built output — the design preview '
          + 'would force sandboxed inline mode: ' + storageHits.join(', '),
        );
      }
      if (requireHits.length) {
        this.error(
          'CommonJS require() left in the built output: ' + requireHits.join(', '),
        );
      }
    },
  };
}

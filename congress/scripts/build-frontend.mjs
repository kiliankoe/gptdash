import * as esbuild from "esbuild";
import JavaScriptObfuscator from "javascript-obfuscator";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, "..");
const staticDir = join(projectRoot, "static");
const jsDir = join(staticDir, "js");
const distDir = join(jsDir, "dist");
const htmlDistDir = join(staticDir, "dist");

const entryPoints = [
  { name: "public", entry: join(jsDir, "audience.js") },
  { name: "beamer", entry: join(jsDir, "beamer.js"), inline: true },
  { name: "player", entry: join(jsDir, "player.js") },
  { name: "host", entry: join(jsDir, "host.js"), inline: true },
];

const obfuscatorOptions = {
  compact: true,
  controlFlowFlattening: true,
  controlFlowFlatteningThreshold: 0.5,
  identifierNamesGenerator: "mangled-shuffled",
  stringArray: true,
  stringArrayEncoding: ["base64"],
  stringArrayThreshold: 0.5,
  renameGlobals: false,
};

async function build() {
  await mkdir(distDir, { recursive: true });
  await mkdir(htmlDistDir, { recursive: true });

  const inlinedJs = {};

  for (const { name, entry, inline } of entryPoints) {
    console.log(`Building ${name}...`);

    const result = await esbuild.build({
      entryPoints: [entry],
      bundle: true,
      format: "iife",
      minify: true,
      write: false,
    });

    const bundled = result.outputFiles[0].text;
    const obfuscated = JavaScriptObfuscator.obfuscate(
      bundled,
      obfuscatorOptions,
    );

    const obfuscatedCode = obfuscated.getObfuscatedCode();

    if (inline) {
      inlinedJs[name] = obfuscatedCode;
      console.log(`  -> (will be inlined into ${name}.html)`);
    } else {
      const outPath = join(distDir, `${name}.min.js`);
      await writeFile(outPath, obfuscatedCode);
      console.log(`  -> ${outPath}`);
    }
  }

  for (const [name, jsCode] of Object.entries(inlinedJs)) {
    console.log(`Inlining JS into ${name}.html...`);

    const htmlPath = join(staticDir, `${name}.html`);
    let html = await readFile(htmlPath, "utf-8");

    const scriptTag = `<script src="/js/dist/${name}.min.js"></script>`;
    const inlineScript = `<script>${jsCode}</script>`;

    if (!html.includes(scriptTag)) {
      throw new Error(
        `Could not find ${scriptTag} in ${name}.html - has the HTML structure changed?`,
      );
    }

    html = html.replace(scriptTag, inlineScript);

    const outPath = join(htmlDistDir, `${name}.html`);
    await writeFile(outPath, html);
    console.log(`  -> ${outPath}`);
  }

  console.log("Frontend build complete.");
}

build().catch((err) => {
  console.error("Build failed:", err);
  process.exit(1);
});

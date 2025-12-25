import * as esbuild from "esbuild";
import JavaScriptObfuscator from "javascript-obfuscator";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, "..");
const staticDir = join(projectRoot, "static");
const jsDir = join(staticDir, "js");
const distDir = join(jsDir, "dist");

const entryPoints = [
	{ name: "public", entry: join(jsDir, "audience.js") },
	{ name: "beamer", entry: join(jsDir, "beamer.js") },
	{ name: "player", entry: join(jsDir, "player.js") },
	{ name: "host", entry: join(jsDir, "host.js") },
];

const obfuscatorOptions = {
	compact: true,
	controlFlowFlattening: true,
	controlFlowFlatteningThreshold: 0.5,
	identifierNamesGenerator: "mangled-shuffled",
	stringArray: false,
	renameGlobals: false,
};

async function build() {
	await mkdir(distDir, { recursive: true });

	for (const { name, entry } of entryPoints) {
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

		const outPath = join(distDir, `${name}.min.js`);
		await writeFile(outPath, obfuscated.getObfuscatedCode());
		console.log(`  -> ${outPath}`);
	}

	console.log("Frontend build complete.");
}

build().catch((err) => {
	console.error("Build failed:", err);
	process.exit(1);
});

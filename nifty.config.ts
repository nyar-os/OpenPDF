// Nifty project configuration.
// See @doki-land/nifty for defineConfig, format presets, authorMap path, and lint rules.
export default {
    trust: {
        npm: {
            repo: "nyar-os/OpenPDF",
            file: "publish-npm.yml",
            environment: "NPM_PUBLISH",
        },
    },
};

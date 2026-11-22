/**
 * Build and release configuration for OpenPDF.
 *
 * Pure literals only: `wae` parses this file statically and never executes it.
 * Build order is fixed: native → ts → staging.
 *
 * The packages live under `projects/packages`; the native addon is built from
 * the `open-pdf-napi` crate and staged into the per-platform packages.
 */
import { defineConfig } from '@wae/wae';

export default defineConfig({
    native: {
        crate: 'open-pdf-napi',
        cdylib: 'open_pdf_napi',
        targets: [
            {
                platform: 'win32',
                arch: 'x64',
                triple: 'x86_64-pc-windows-msvc',
                package: '@wae/open-pdf-win32-x64',
                nodeFile: 'open-pdf.win32-x64.node',
                dir: 'projects/packages/open-pdf-win32-x64',
            },
            {
                platform: 'darwin',
                arch: 'x64',
                triple: 'x86_64-apple-darwin',
                package: '@wae/open-pdf-darwin-x64',
                nodeFile: 'open-pdf.darwin-x64.node',
                dir: 'projects/packages/open-pdf-darwin-x64',
            },
            {
                platform: 'darwin',
                arch: 'arm64',
                triple: 'aarch64-apple-darwin',
                package: '@wae/open-pdf-darwin-arm64',
                nodeFile: 'open-pdf.darwin-arm64.node',
                dir: 'projects/packages/open-pdf-darwin-arm64',
            },
            {
                platform: 'linux',
                arch: 'x64',
                triple: 'x86_64-unknown-linux-gnu',
                package: '@wae/open-pdf-linux-x64',
                nodeFile: 'open-pdf.linux-x64.node',
                dir: 'projects/packages/open-pdf-linux-x64',
            },
            {
                platform: 'linux',
                arch: 'arm64',
                triple: 'aarch64-unknown-linux-gnu',
                package: '@wae/open-pdf-linux-arm64',
                nodeFile: 'open-pdf.linux-arm64.node',
                dir: 'projects/packages/open-pdf-linux-arm64',
            },
        ],
    },
    packages: [
        {
            name: '@wae/open-pdf-win32-x64',
            dir: 'projects/packages/open-pdf-win32-x64',
            os: ['win32'],
            cpu: ['x64'],
            main: 'open-pdf.win32-x64.node',
        },
        {
            name: '@wae/open-pdf-darwin-x64',
            dir: 'projects/packages/open-pdf-darwin-x64',
            os: ['darwin'],
            cpu: ['x64'],
            main: 'open-pdf.darwin-x64.node',
        },
        {
            name: '@wae/open-pdf-darwin-arm64',
            dir: 'projects/packages/open-pdf-darwin-arm64',
            os: ['darwin'],
            cpu: ['arm64'],
            main: 'open-pdf.darwin-arm64.node',
        },
        {
            name: '@wae/open-pdf-linux-x64',
            dir: 'projects/packages/open-pdf-linux-x64',
            os: ['linux'],
            cpu: ['x64'],
            main: 'open-pdf.linux-x64.node',
        },
        {
            name: '@wae/open-pdf-linux-arm64',
            dir: 'projects/packages/open-pdf-linux-arm64',
            os: ['linux'],
            cpu: ['arm64'],
            main: 'open-pdf.linux-arm64.node',
        },
    ],
    build: {
        ts: { packages: ['@wae/open-pdf'] },
        outputs: ['projects/packages/open-pdf/dist/cli.js'],
    },
    ci: {
        repo: 'nyar-os/OpenPDF',
        workflows: ['rust.yml'],
        requiredJobs: ['build'],
        shaBinding: false,
    },
});

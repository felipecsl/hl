import { HLConfig } from "./config.js";
export declare function buildAndPush(opts: {
    context: string;
    dockerfile?: string;
    tags: string[];
    platforms?: string;
}): Promise<void>;
export declare function retagLatest(image: string, fromTag: string): Promise<void>;
export declare function restartCompose(cfg: HLConfig): Promise<void>;
export declare function runMigrations(cfg: HLConfig, imageTag: string): Promise<void>;
export declare function inspectContainerNetworks(name: string): Promise<string>;
export declare function tagFor(cfg: HLConfig, sha: string, branch: string): {
    sha: string;
    branchSha: string;
    latest: string;
};
export declare function composePath(cfg: HLConfig): string;
//# sourceMappingURL=docker.d.ts.map
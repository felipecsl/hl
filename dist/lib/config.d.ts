import { z } from "zod";
export declare const HL_ROOT = "/home/felipecsl/prj/apps";
export declare const ConfigSchema: z.ZodObject<{
    app: z.ZodString;
    image: z.ZodString;
    domain: z.ZodString;
    servicePort: z.ZodNumber;
    resolver: z.ZodDefault<z.ZodString>;
    network: z.ZodDefault<z.ZodString>;
    platforms: z.ZodDefault<z.ZodString>;
    health: z.ZodObject<{
        url: z.ZodString;
        interval: z.ZodDefault<z.ZodString>;
        timeout: z.ZodDefault<z.ZodString>;
    }, "strip", z.ZodTypeAny, {
        url: string;
        interval: string;
        timeout: string;
    }, {
        url: string;
        interval?: string | undefined;
        timeout?: string | undefined;
    }>;
    migrations: z.ZodObject<{
        command: z.ZodDefault<z.ZodArray<z.ZodString, "many">>;
        env: z.ZodDefault<z.ZodRecord<z.ZodString, z.ZodString>>;
    }, "strip", z.ZodTypeAny, {
        command: string[];
        env: Record<string, string>;
    }, {
        command?: string[] | undefined;
        env?: Record<string, string> | undefined;
    }>;
    secrets: z.ZodDefault<z.ZodArray<z.ZodString, "many">>;
}, "strip", z.ZodTypeAny, {
    app: string;
    image: string;
    domain: string;
    servicePort: number;
    resolver: string;
    network: string;
    platforms: string;
    health: {
        url: string;
        interval: string;
        timeout: string;
    };
    migrations: {
        command: string[];
        env: Record<string, string>;
    };
    secrets: string[];
}, {
    app: string;
    image: string;
    domain: string;
    servicePort: number;
    health: {
        url: string;
        interval?: string | undefined;
        timeout?: string | undefined;
    };
    migrations: {
        command?: string[] | undefined;
        env?: Record<string, string> | undefined;
    };
    resolver?: string | undefined;
    network?: string | undefined;
    platforms?: string | undefined;
    secrets?: string[] | undefined;
}>;
export type HLConfig = z.infer<typeof ConfigSchema>;
export declare function loadConfig(app: string): Promise<HLConfig>;
export declare const appDir: (app: string) => string;
export declare const envFile: (app: string) => string;
export declare function parseDuration(s: string): number;
//# sourceMappingURL=config.d.ts.map
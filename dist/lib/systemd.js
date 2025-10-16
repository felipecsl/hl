import { execa } from "execa";
import { promises as fs } from "fs";
import { appDir } from "./config.js";
export async function writeUnit(app) {
    const unit = `app-${app}.service`;
    const wd = appDir(app);
    const text = `[Unit]
Description=Compose stack for ${app}
After=docker.service
Requires=docker.service

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory=${wd}
ExecStart=/usr/bin/docker compose -f compose.yml up -d
ExecStop=/usr/bin/docker compose -f compose.yml down
TimeoutStartSec=0

[Install]
WantedBy=multi-user.target
`;
    await fs.writeFile(`/etc/systemd/system/${unit}`, text);
    await execa("systemctl", ["daemon-reload"], { stdio: "inherit" });
    await execa("systemctl", ["enable", "--now", unit], { stdio: "inherit" });
    return unit;
}
//# sourceMappingURL=systemd.js.map
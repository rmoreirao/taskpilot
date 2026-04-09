fn main() {
    #[cfg(windows)]
    {
        let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());

        // Strip prerelease/build metadata (e.g. "0.2.0-beta.1+build" → "0.2.0")
        let semver_core = version.split(['-', '+']).next().unwrap_or("0.0.0");
        let parts: Vec<u16> = semver_core
            .split('.')
            .map(|s| s.parse().unwrap_or(0))
            .collect();
        let major = *parts.first().unwrap_or(&0) as u64;
        let minor = *parts.get(1).unwrap_or(&0) as u64;
        let patch = *parts.get(2).unwrap_or(&0) as u64;
        let win_version_str = format!("{major}.{minor}.{patch}.0");
        let win_version_bin = (major << 48) | (minor << 32) | (patch << 16);

        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "TaskPilot");
        res.set("FileDescription", "TaskPilot \u{2013} Lightweight Windows Task Scheduler");
        res.set("CompanyName", "TaskPilot");
        res.set("LegalCopyright", "\u{00A9} TaskPilot contributors");
        res.set("FileVersion", &win_version_str);
        res.set("ProductVersion", &win_version_str);
        res.set_version_info(winresource::VersionInfo::FILEVERSION, win_version_bin);
        res.set_version_info(winresource::VersionInfo::PRODUCTVERSION, win_version_bin);
        res.compile().expect("Failed to compile Windows resources");
    }
}

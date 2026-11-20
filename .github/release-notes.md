## v0.15.1 更新内容 / What's new

- **每个整点检查更新**：应用开着时，每个整点（本机时间）自动检查一次新版本，发版后最迟到下一个整点就会在顶栏提示；启动后和窗口切回来时，如果这个小时还没查过也会查一次。检查失败下个整点再试；电脑睡眠错过整点时，切回窗口即补查。之前是距上次检查满一小时才查，时间不好预期。
- 本版是用 v0.15.0 的规则发现的（应用开着时最迟约一小时）；装上本版后，从下一个版本起按整点提示。
- 装着 v0.13.0 及之后版本的应用可以在应用内一键更新；v0.12.0 及更早的版本仍需手动下载安装。

- **Update checks on every hour**: while the app is open it checks for a new version on every hour (local time), so a release shows in the toolbar by the next full hour at the latest; at startup and when you switch back to its window, it also checks if this hour has not been checked yet. A failed check is retried on the next hour, and if the computer slept through an hour, switching back to the window catches up. Previously it checked once an hour had passed since the last check, which was hard to predict.
- This version is found by v0.15.0's rule (within about an hour while the app is open); once installed, later versions are announced on the hour.
- Apps on v0.13.0 or later can update in-app; v0.12.0 and earlier still need a manual install.

//! When to stop recording: once the desktop is ready and the settle time has passed,
//! or at the timeout.

use kokuin_kb::ProcessRole;
use kokuin_model::EndReason;
use kokuin_sources::is_normal_user;

/// Decides when the recording ends. Times are on the boot clock, in ns.
#[derive(Debug, Clone)]
pub struct Watch {
    settle_ns: i64,
    timeout_ns: i64,
    ready_ns: Option<i64>,
}

impl Watch {
    /// Records until `settle_ns` after the desktop is ready, and never after
    /// `timeout_ns` (counted from the kernel start).
    pub fn new(settle_ns: i64, timeout_ns: i64) -> Self {
        Self {
            settle_ns,
            timeout_ns,
            ready_ns: None,
        }
    }

    /// A program started at `t_ns`, with this role, for this user. The desktop is
    /// ready when a user's session shell runs (a panel, a bar, or a compositor that
    /// is its own shell, like GNOME Shell).
    pub fn observe(&mut self, t_ns: i64, role: Option<ProcessRole>, uid: Option<u32>) {
        let shell = matches!(
            role,
            Some(ProcessRole::SessionShell | ProcessRole::CompositorShell)
        );
        if self.ready_ns.is_none() && shell && is_normal_user(uid) {
            self.ready_ns = Some(t_ns);
        }
    }

    /// When the desktop became ready.
    pub fn ready_ns(&self) -> Option<i64> {
        self.ready_ns
    }

    /// When the recording will stop, as far as is known now.
    pub fn stop_ns(&self) -> i64 {
        match self.ready_ns {
            Some(ready) => (ready + self.settle_ns).min(self.timeout_ns),
            None => self.timeout_ns,
        }
    }

    /// Why to stop now, if it is time.
    pub fn end_reason(&self, now_ns: i64) -> Option<EndReason> {
        if now_ns < self.stop_ns() {
            return None;
        }
        Some(match self.ready_ns {
            Some(ready) if ready + self.settle_ns <= self.timeout_ns => EndReason::SessionReady,
            _ => EndReason::Timeout,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: i64 = 1_000_000_000;

    #[test]
    fn stops_after_the_settle_time_once_the_desktop_is_ready() {
        let mut watch = Watch::new(30 * S, 180 * S);
        // A display manager runs as root: it is not the desktop.
        watch.observe(10 * S, Some(ProcessRole::DisplayManager), Some(0));
        // The compositor alone is not enough; its bar is.
        watch.observe(13 * S, Some(ProcessRole::Compositor), Some(1000));
        assert_eq!(watch.end_reason(40 * S), None);
        watch.observe(14 * S, Some(ProcessRole::SessionShell), Some(1000));
        assert_eq!(watch.stop_ns(), 44 * S);
        assert_eq!(watch.end_reason(43 * S), None);
        assert_eq!(watch.end_reason(44 * S), Some(EndReason::SessionReady));
    }

    #[test]
    fn gnome_shell_is_its_own_shell() {
        let mut watch = Watch::new(30 * S, 180 * S);
        watch.observe(19 * S, Some(ProcessRole::CompositorShell), Some(1000));
        assert_eq!(watch.ready_ns(), Some(19 * S));
    }

    #[test]
    fn the_login_screen_shell_does_not_count() {
        // GDM runs its own gnome-shell as the `gdm` system user.
        let mut watch = Watch::new(30 * S, 180 * S);
        watch.observe(12 * S, Some(ProcessRole::CompositorShell), Some(120));
        assert_eq!(watch.ready_ns(), None);
    }

    #[test]
    fn a_system_without_desktop_stops_at_the_timeout() {
        let watch = Watch::new(30 * S, 180 * S);
        assert_eq!(watch.end_reason(179 * S), None);
        assert_eq!(watch.end_reason(180 * S), Some(EndReason::Timeout));
    }

    #[test]
    fn a_desktop_ready_too_late_still_stops_at_the_timeout() {
        let mut watch = Watch::new(30 * S, 180 * S);
        watch.observe(170 * S, Some(ProcessRole::SessionShell), Some(1000));
        assert_eq!(watch.end_reason(180 * S), Some(EndReason::Timeout));
    }
}

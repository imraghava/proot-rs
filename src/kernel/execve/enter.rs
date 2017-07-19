use nix::unistd::Pid;
use nix::errno::Errno;
use errors::{Result, Error};
use register::{Registers, Word};
use filesystem::fs::FileSystem;
use filesystem::translation::Translator;
use process::tracee::Tracee;
use kernel::sysarg::get_sysarg_path;
use kernel::execve::{shebang, load_info};

pub fn translate(pid: Pid, fs: &FileSystem, tracee: &mut Tracee, regs: &Registers) -> Result<()> {
    //	if (IS_NOTIFICATION_PTRACED_LOAD_DONE(tracee)) {
    //		/* Syscalls can now be reported to its ptracer.  */
    //		tracee->as_ptracee.ignore_loader_syscalls = false;
    //
    //		/* Cancel this spurious kernel.execve, it was only used as a
    //		 * notification.  */
    //		set_sysnum(tracee, PR_void);
    //		return 0;
    //	}

    let raw_path = get_sysarg_path(pid, regs.sys_arg_1 as *mut Word)?;
    //TODO: return user path
    let host_path = match shebang::expand(fs, &raw_path) {
        Ok(path) => path,
        // The Linux kernel actually returns -EACCES when trying to execute a directory.
        Err(Error::Sys(Errno::EISDIR)) => return Err(Error::from(Errno::EACCES)),
        Err(error) => return Err(error),
    };

    //	/* user_path is modified only if there's an interpreter
    //	 * (ie. for a script or with qemu).  */
    //	if (status == 0 && tracee->qemu == NULL)
    //		TALLOC_FREE(raw_path);

    //	Remember the new value for "/proc/self/exe".  It points to
    //	a canonicalized guest path, hence detranslate_path()
    //	instead of using user_path directly.  */
    if let Ok(maybe_path) = fs.detranslate_path(&host_path, None) {
        tracee.set_new_exec(Some(maybe_path.unwrap_or(host_path.clone())));
    } else {
        tracee.set_new_exec(None);
    }

    //	if (tracee->qemu != NULL) {
    //		status = expand_runner(tracee, host_path, user_path);
    //		if (status < 0)
    //			return status;
    //	}

    let mut load_info = load_info::LoadInfo::new();

    load_info.raw_path = Some(raw_path.clone());
    load_info.host_path = Some(host_path.clone());
    //TODO: use user_path when implemented
    load_info.user_path = Some(raw_path.clone());

    let complete_load_info = load_info::extract(&host_path, &mut load_info)?;

    //	status = extract_load_info(tracee, tracee->load_info);
    //	if (status < 0)
    //		return status;
    //
    //	if (tracee->load_info->interp != NULL) {
    //		status = extract_load_info(tracee, tracee->load_info->interp);
    //		if (status < 0)
    //			return status;
    //
    //		/* An ELF interpreter is supposed to be
    //		 * standalone.  */
    //		if (tracee->load_info->interp->interp != NULL)
    //			return -EINVAL;
    //	}
    //
    //	compute_load_addresses(tracee);
    //
    //	/* Execute the loader instead of the program.  */
    //	loader_path = get_loader_path(tracee);
    //	if (loader_path == NULL)
    //		return -ENOENT;
    //
    //	status = set_sysarg_path(tracee, loader_path, SYSARG_1);
    //	if (status < 0)
    //		return status;
    //
    //	/* Mask to its ptracer kernel performed by the loader.  */
    //	tracee->as_ptracee.ignore_loader_syscalls = true;
    //
    //	return 0;

    Ok(())
}

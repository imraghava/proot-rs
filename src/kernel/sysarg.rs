use std::mem::{size_of, transmute};
use std::ptr::null_mut;
use std::path::PathBuf;
use libc::{c_void, PATH_MAX};
use errors::Result;
use errors::Error;
use nix::unistd::Pid;
use nix::sys::ptrace::ptrace;
use nix::sys::ptrace::ptrace::PTRACE_PEEKDATA;
use register::Word;

/// Retrieves a path from one of the syscall's arguments.
///
/// * `pid` is the process ID of the tracee
/// * `src_sysarg` is the result of `get_reg` applied to one of the registers.
///    It contains the address of the path's string in the memory space of the tracee.
#[inline]
pub fn get_sysarg_path(pid: Pid, src_sysarg: *mut Word) -> Result<PathBuf> {
    if src_sysarg.is_null() {
        /// Check if the parameter is not NULL. Technically we should
        /// not return an error for this special value since it is
        /// allowed for some kernel, utimensat(2) for instance.
        Ok(PathBuf::new())
    } else {
        /// Get the path from the tracee's memory space.
        read_path(pid, src_sysarg)
    }
}

/// Intermediary function that retrieves bytes from the tracee's memory space
/// and collects them into a null-terminated CString.
///
/// It also checks that the number of bytes isn't too long.
#[inline]
fn read_path(pid: Pid, src_path: *mut Word) -> Result<PathBuf> {
    let bytes = read_string(pid, src_path, PATH_MAX as usize)?;

    if bytes.len() >= PATH_MAX as usize {
        return Err(Error::name_too_long("when reading sys arg path"));
    }

    Ok(PathBuf::from(unsafe { String::from_utf8_unchecked(bytes) }))
}

/// Reads a string from the memory space of a tracee.
///
/// It uses `ptrace(PEEK_DATA)` to read it word by word
/// (1 word = 1 c_ulong = 1 u32 or 1 u64 = 4 or 8 u8 = 4 or 8 char).
/// The copy stops when a null character `\0` is encountered,
/// The bytes contained at the string's address are returned as a Vector of u8.
///
/// * `pid` is the pid of the tracee.
/// * `src_string` is the address of the string in tracee's memory space
///     (obtained for instance with `get_reg`).
/// * `max_size` is the maximum number of bytes copied from memory.
fn read_string(pid: Pid, src_string: *mut Word, max_size: usize) -> Result<Vec<u8>> {
    let mut bytes: Vec<u8> = Vec::with_capacity(max_size);

    // if (belongs_to_heap_prealloc(tracee, dest_tracee))
    //	return -EFAULT;

    //todo: HAVE_PROCESS_VM (when necessary)

    let word_size = size_of::<Word>();
    let nb_trailing_bytes = (max_size % word_size) as isize;
    let nb_full_words = ((max_size - nb_trailing_bytes as usize) / word_size) as isize;

    // Copy one word by one word, except for the last one.
    for i in 0..nb_full_words {
        // ptrace returns a c_long/Word that we will interpret as an 8-letters word
        let word = ptrace(
            PTRACE_PEEKDATA,
            pid,
            unsafe { src_string.offset(i) as *mut c_void },
            null_mut(),
        )? as Word;
        //TODO: find a way to do thing for 4 bytes (32bits procs)
        let letters = convert_word_to_bytes(word);

        for &letter in &letters {
            // Stop once an end-of-string is detected.
            if letter as char == '\0' {
                bytes.shrink_to_fit();

                // No need to add the \0 null character now,
                // as it will be added when converting the bytes in a CString.
                return Ok(bytes);
            }
            bytes.push(letter);
        }
    }

    //todo: add trailing bytes processing (when necessary, need an example where it's actually used)
    unimplemented!("trailing bytes not supported!")

    /*

	/* Copy the bytes from the last word carefully since we have
	 * to not overwrite the bytes lying beyond @dest_tracer. */

	word = ptrace(PTRACE_PEEKDATA, tracee->pid, src + i, NULL);
	if (errno != 0)
		return -EFAULT;

	dest_word = (uint8_t *)&dest[i];
	src_word  = (uint8_t *)&word;

	for (j = 0; j < nb_trailing_bytes; j++) {
		dest_word[j] = src_word[j];
		if (src_word[j] == '\0')
			break;
	}

	return i * sizeof(word_t) + j + 1;
    */
}

#[cfg(target_pointer_width = "32")]
#[inline]
fn convert_word_to_bytes(value_to_convert: Word) -> [u8; 4] {
    unsafe { transmute(value_to_convert) }
}

#[cfg(target_pointer_width = "64")]
#[inline]
fn convert_word_to_bytes(value_to_convert: Word) -> [u8; 8] {
    unsafe { transmute(value_to_convert) }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;
    use std::ffi::CString;
    use nix::unistd::execvp;
    use utils::tests::fork_test;
    use syscall::nr::MKDIR;
    use register::Word;

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn test_sysarg_convert_word_to_bytes() {
        let number: Word = 'h' as u64 + 'e' as u64 * 256 + 'l' as u64 * 256 * 256 +
            'l' as u64 * 256 * 256 * 256 +
            'o' as u64 * 256 * 256 * 256 * 256;
        let bytes = convert_word_to_bytes(number);

        assert_eq!(
            bytes,
            [
                'h' as u8,
                'e' as u8,
                'l' as u8,
                'l' as u8,
                'o' as u8,
                0,
                0,
                0,
            ]
        );
    }

    #[test]
    #[cfg(target_pointer_width = "32")]
    fn test_sysarg_convert_word_to_bytes() {
        let number: Word = 'h' as u64 + 'e' as u64 * 256 + 'l' as u64 * 256 * 256 +
            'o' as u64 * 256 * 256 * 256;
        let bytes = convert_word_to_bytes(number);

        assert_eq!(bytes, ['h' as u8, 'e' as u8, 'l' as u8, 'o' as u8]);
    }

    #[test]
    fn test_sysarg_get_sysarg_path_return_empty_if_given_null_src_() {
        let path = get_sysarg_path(Pid::from_raw(0), null_mut()).unwrap();

        assert_eq!(path.to_str().unwrap(), "");
    }

    #[test]
    /// Tests that `get_sysarg_path`, `read_path` and `read_string` all work on a simple syscall,
    /// and succeeds in reading a syscall's path argument.
    ///
    /// The test is a success if the MKDIR syscall is detected (with its corresponding signum),
    /// and if the first argument of the syscall correspond to the path given to the initial command.
    fn test_sysarg_get_sysarg_path_for_mkdir_test() {
        let test_path = "my/impossible/test/path";

        fork_test(
            // expecting an error (because the path doesn't exit)
            1,
            // parent
            |pid, regs| {
                if regs.sys_num == MKDIR {
                    let dir_path = get_sysarg_path(pid, regs.sys_arg_1 as *mut Word).unwrap();

                    // we're checking that the string read in the tracee's memory
                    // corresponds to what has been given to the execve command
                    assert_eq!(dir_path, PathBuf::from(test_path));

                    // we can stop here
                    return true;
                } else {
                    return false;
                }
            },
            // child
            || {
                // calling the mkdir function, which should call the MKDIR syscall
                execvp(
                    &CString::new("mkdir").unwrap(),
                    &[CString::new(".").unwrap(), CString::new(test_path).unwrap()],
                ).expect("failed execvp mkdir");
            },
        );
    }
}

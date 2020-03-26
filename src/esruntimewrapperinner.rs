use crate::es_utils::EsErrorInfo;
use crate::esvaluefacade::EsValueFacade;
use crate::microtaskmanager::MicroTaskManager;
use crate::spidermonkeyruntimewrapper::SmRuntime;
use log::{debug, trace};
use std::sync::Arc;

pub type ImmutableJob<R> = Box<dyn FnOnce(&SmRuntime) -> R + Send + 'static>;
pub type MutableJob<R> = Box<dyn FnOnce(&mut SmRuntime) -> R + Send + 'static>;

pub struct EsRuntimeWrapperInner {
    pub(crate) task_manager: Arc<MicroTaskManager>,
}

impl EsRuntimeWrapperInner {
    pub fn call(&self, function_name: &str, args: Vec<EsValueFacade>) -> () {
        debug!("call {} in thread {}", function_name, thread_id::get());
        let f_n = function_name.to_string();
        self.do_in_es_runtime_thread(Box::new(move |sm_rt: &SmRuntime| {
            let res = sm_rt.call(f_n.as_str(), args);
            if res.is_err() {
                debug!("async call failed: {}", res.err().unwrap().message);
            }
        }))
    }

    pub fn call_sync(
        &self,
        function_name: &str,
        args: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        trace!("call_sync {} in thread {}", function_name, thread_id::get());
        let f_n = function_name.to_string();
        self.do_in_es_runtime_thread_sync(Box::new(move |sm_rt: &SmRuntime| {
            sm_rt.call(f_n.as_str(), args)
        }))
    }

    pub fn eval(&self, eval_code: &str, file_name: &str) -> () {
        debug!("eval {} in thread {}", eval_code, thread_id::get());

        let eval_code = eval_code.to_string();
        let file_name = file_name.to_string();

        self.do_in_es_runtime_thread(Box::new(move |sm_rt: &SmRuntime| {
            let res = sm_rt.eval(eval_code.as_str(), file_name.as_str());
            if res.is_err() {
                debug!("async code eval failed: {}", res.err().unwrap().message);
            }
        }))
    }

    pub fn eval_sync(&self, code: &str, file_name: &str) -> Result<EsValueFacade, EsErrorInfo> {
        debug!("eval_sync1 {} in thread {}", code, thread_id::get());
        let eval_code = code.to_string();
        let file_name = file_name.to_string();

        self.do_in_es_runtime_thread_sync(Box::new(move |sm_rt: &SmRuntime| {
            sm_rt.eval(eval_code.as_str(), file_name.as_str())
        }))
    }

    pub(crate) fn cleanup_sync(&self) {
        trace!("cleaning up es_rt");
        // todo, set is_cleaning var on inner, here and now
        // that should hint the engine to not use this runtime
        self.do_in_es_runtime_thread_sync(Box::new(move |sm_rt: &SmRuntime| {
            sm_rt.cleanup();
        }));
        // reset cleaning var here
    }

    pub fn do_in_es_runtime_thread(&self, immutable_job: ImmutableJob<()>) -> () {
        trace!("do_in_es_runtime_thread");
        // this is executed in the single thread in the Threadpool, therefore Runtime and global are stored in a thread_local

        let job = || {
            let ret = crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt| {
                debug!("got rt from thread_local");
                immutable_job(&mut sm_rt.borrow())
            });

            return ret;
        };

        self.task_manager.add_task(job);
    }
    pub fn do_in_es_runtime_thread_sync<R: Send + 'static>(
        &self,
        immutable_job: ImmutableJob<R>,
    ) -> R {
        trace!("do_in_es_runtime_thread_sync");
        // this is executed in the single thread in the Threadpool, therefore Runtime and global are stored in a thread_local

        let job = || {
            let ret = crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt| {
                debug!("got rt from thread_local");
                immutable_job(&mut sm_rt.borrow())
            });

            ret
        };

        self.task_manager.exe_task(job)
    }

    pub fn do_in_es_runtime_thread_mut_sync(&self, mutable_job: MutableJob<()>) -> () {
        trace!("do_in_es_runtime_thread_mut_sync");
        // this is executed in the single thread in the Threadpool, therefore Runtime and global are stored in a thread_local

        let job = || {
            let ret = crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt| {
                debug!("got rt from thread_local");
                mutable_job(&mut sm_rt.borrow_mut())
            });

            return ret;
        };

        self.task_manager.exe_task(job);
    }
    pub(crate) fn register_op(
        &self,
        name: &'static str,
        op: crate::spidermonkeyruntimewrapper::OP,
    ) {
        self.do_in_es_runtime_thread_mut_sync(Box::new(move |sm_rt: &mut SmRuntime| {
            sm_rt.register_op(name, op);
        }));
    }
}

impl Drop for EsRuntimeWrapperInner {
    fn drop(&mut self) {
        self.do_in_es_runtime_thread_mut_sync(Box::new(|_sm_rt: &mut SmRuntime| {
            debug!("dropping EsRuntimeWrapperInner");
        }));
    }
}
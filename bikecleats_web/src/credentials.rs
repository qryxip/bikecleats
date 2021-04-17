use crate::shell::Shell;

//pub struct UsernameAndPassword<'a>(&'a mut dyn FnMut() -> anyhow::Result<(String, String)>);
//
//impl<'a> UsernameAndPassword<'a> {
//    pub fn new<
//        U: 'a + FnMut() -> anyhow::Result<String>,
//        P: 'a + FnMut() -> anyhow::Result<String>,
//    >(
//        username: U,
//        password: P,
//    ) -> Self {
//        Self(&mut move || Ok((username()?, password()?)))
//    }
//}

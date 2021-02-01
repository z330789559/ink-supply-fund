#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;

#[ink::contract(dynamic_storage_allocator = true)]
mod scm_finance {
    
    #[cfg(not(std))]
    use ink_env::{
        debug_println,
        call::{build_create,ExecutionInput,Selector,build_call,utils::ReturnType},
        DefaultEnvironment,
        hash::{
            Blake2x256,
            CryptoHash,
            HashOutput,
        },
        Clear,
    };
    use ink_prelude::{
        string::String
    };
    use ink_storage::{
        collections::HashMap as StorageHashMap,
        collections::hashmap::Entry,
        Vec as StorageVec,
        Box as StorageBox,
        Pack,
        traits::{
            PackedLayout,
            SpreadLayout,
        },
        Lazy,
    };

    use erc20::Erc20;
    use ctoken::Ctoken;
    use ink_env::call::FromAccountId;
    /// The ERC-20 error types.
    #[derive(Debug, PartialEq, Eq, scale::Encode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        InsufficientExitSwap,
        InsufficientAdmin,
        InsufficientBalance,
        InsufficientInterest,
        OnlyOwner,
        InsufficientTokenUsed,
        InsufficientInterval,
        InsufficientRewards,
        InsufficientRemainRewards,
        InsufficientSwapPool,
        InsufficientSwapConfig,
        InsufficientSwapCorrelation,
    }


    #[ink(event)]
    pub struct SwapToken{
        #[ink(topic)]
        pub swap_token:AccountId,
        #[ink(topic)]
        pub origin_token:AccountId,
        #[ink(topic)]
        pub prince: u128
    }

  /// The ERC-20 result type.
  pub type MyResult<T> = core::result::Result<T, Error>;
    /// Defines the storage of your contract.
    /// Add new fields to the below struct in order
    /// to add new static storage fields to your contract.
    #[ink(storage)]
    #[derive(Default)]
    pub struct ScmFinance {
      ///交易池
        swaps: StorageHashMap<String,(AccountId,AccountId)>,
         
        /// Stores a single `bool` value on the storage.
        admin: AccountId,

        token_swap: StorageHashMap<AccountId,String>,

        swap_token: StorageHashMap<String,AccountId>,

        interest: StorageHashMap<String,u32>,

        rewards: StorageHashMap<String,Balance>,

        interval: StorageHashMap<String,u32>,

        schedule: StorageHashMap<String,BlockNumber>,
//动态存储
     
    }

    impl ScmFinance {
        /// Constructor that initializes the `bool` value to the given `init_value`.
        #[ink(constructor)]
        pub fn new() -> Self {
          let admin =Self::env().caller();

          Self{
            swaps:StorageHashMap::new(),
            admin:admin,
            token_swap:StorageHashMap::new(),
            interest: StorageHashMap::new(),
            rewards : StorageHashMap::new(),
            interval: StorageHashMap::new(),
            schedule:  StorageHashMap::new(),
            swap_token:  StorageHashMap::new(),
          }
        }
  /// Constructors can delegate to other constructors.

        /// Constructor that initializes the `bool` value to `false`.
        ///
        /// Constructors can delegate to other constructors.
        #[ink(message)]
        pub fn create_swap(&mut self, symbol: String,swap_token: AccountId,token_id: AccountId)->MyResult<()>{
              ///第一步  验证token_id是不是已经创建,
              debug_println("create_swap");
           let tokenName=   self.swaps.get(&symbol);

           if(tokenName != None ){

               return Err(Error::InsufficientExitSwap);
           }

           let _token_exit=self.token_swap.get(&token_id);
           if(_token_exit!=None){
            return Err(Error::InsufficientTokenUsed);
           }
           let caller = Self::env().caller();

           if(caller!= self.admin){
             return Err(Error::InsufficientAdmin);
           } 
           self.token_swap.insert(token_id,symbol.clone());
           self.insert_swap(symbol.clone(),swap_token.clone(),token_id);
           self.swap_token.insert(symbol.clone(),swap_token.clone());
           Ok(())
        }

        #[ink(message)]
        pub fn update_swaps(&mut self,name: String, interest: u32,rewards: Balance,interval: u32)->Result<(),Error>{
         let callee= Self::env().caller();
         if(interest >1000){
             return Err(Error::InsufficientInterest)
         }
         if rewards==0 {
            return Err(Error::InsufficientBalance)
         }
         if interval==0 {
            return Err(Error::InsufficientInterval)
         }

         self.interest.entry(name.clone()).or_insert(interest);
         self.interval.entry(name.clone()).or_insert(interval);

       let mut _rewrads=  self.rewards.entry(name.clone()).or_insert(0);
            *_rewrads+=rewards;
          Ok(())
        }

        fn only_owner(&self, caller: AccountId) -> Result<(),Error> {
            if self.admin == caller {
                Ok(())
            } else {
                return Err(Error::OnlyOwner);
            }
        }

        /**
         * 发布新的融资
         */
        #[ink(message)]
        pub fn minit(&mut self, amount: Balance,to: AccountId, name: String)->Result<(),Error>{
            let callee= Self::env().caller();

            self.only_owner(callee)?;
            if(amount <0 ){
                return Err(Error::InsufficientBalance)
            }
             let _contratc=  Self::env().account_id();
           ///查询融资池
          let _swap= self.get_token_swap_or_default(name);
            if let Err(a)=_swap{
            return Err(Error::InsufficientExitSwap)
           }

           let (ref _swap_token ,ref _origin_token):(AccountId,AccountId)=_swap.unwrap();
            let mut _swap_token_erc20: Ctoken = FromAccountId::from_account_id((*_swap_token).clone());
         let mut _origin_token_erc20: Erc20 = FromAccountId::from_account_id((*_origin_token).clone());
           let _swap_token_supply= _swap_token_erc20.total_supply();
           let _orgin_token_supply= _origin_token_erc20.total_supply();
            let price =self.cal_price(_swap_token_supply.into(), _orgin_token_supply.into());
            let _add_swap_token = price.saturating_mul(amount).wrapping_div(1000u128);
    
              _origin_token_erc20.minit(to,amount);
             _swap_token_erc20.minit(_contratc,_add_swap_token.into());
             self.env().emit_event(SwapToken{
                swap_token: (*_swap_token).clone(),
                origin_token: (*_origin_token).clone(),
                prince: price
            });
           Ok(())
        }

       /**
         * 回收融资
         */
        #[ink(message)]
        pub fn burn(&mut self,name: String, amount: Balance,accountId: AccountId)->Result<(),Error>{
            if(amount <0 ){
                return Err(Error::InsufficientBalance)
            }
             let _contratc=  Self::env().account_id();
           ///查询融资池
          let _swap= self.get_token_swap_or_default(name);
            if let Err(a)=_swap{
            return Err(Error::InsufficientExitSwap)
           }
           let (ref _swap_token ,ref _origin_token):(AccountId,AccountId)=_swap.unwrap();
           let mut _swap_token_erc20: Ctoken = FromAccountId::from_account_id((*_swap_token).clone());
           let mut _origin_token_erc20: Erc20 = FromAccountId::from_account_id((*_origin_token).clone());

            let current_amount=  _origin_token_erc20.balance_of_or_zero(accountId);
              if(current_amount <amount ){
                if(amount <0 ){
                    return Err(Error::InsufficientBalance)
                }
              }
              let _swap_token_supply= _swap_token_erc20.total_supply();
              let _orgin_token_supply= _origin_token_erc20.total_supply();
               let price =self.cal_price(_swap_token_supply.into(), _orgin_token_supply.into());
               let _add_swap_token = price.saturating_mul(amount).wrapping_div(1000u128);

               self.env().emit_event(SwapToken{
                swap_token: (*_swap_token).clone(),
                origin_token: (*_origin_token).clone(),
                prince: price
            });
              _origin_token_erc20.burn(accountId,amount);
             _swap_token_erc20.burn(_contratc,_add_swap_token.into());

            Ok(())
        }

       #[ink(message)]
        pub fn add_swap_token(&mut self,amount :Balance,name:String)->Result<(),Error>{
            let caller=Self::env().caller();
               self.only_owner(caller)?;
               if(amount <0 ){
                return Err(Error::InsufficientBalance)
            }
             let _contratc=  Self::env().account_id();
           ///查询融资池
          let _swap= self.get_token_swap_or_default(name);
            if let Err(a)=_swap{
            return Err(Error::InsufficientExitSwap)
           }
           let (ref _swap_token ,ref _origin_token):(AccountId,AccountId)=_swap.unwrap();
           let mut _swap_token_erc20: Ctoken = FromAccountId::from_account_id((*_swap_token).clone());
           _swap_token_erc20.minit(_contratc, amount);
            Ok(())
        }

         #[ink(message)]
       pub fn cal_price(&self, swap_token_amount: u128,origin_token_amount: u128) -> u128 {
          let mut _price= swap_token_amount.saturating_mul( 1000u128 ).wrapping_div(origin_token_amount);
          if(_price ==0u128){
            _price=1u128;
          }
          _price
        }

        #[ink(message)]
        pub fn get_price(&self,name:String)->Result<(Balance,Balance,u128),Error>{

                    ///查询融资池
         let _swap= self.get_token_swap_or_default(name);
         if let Err(a)=_swap{
            return Err(Error::InsufficientExitSwap)
         }

         let (ref _swap_token,ref _origin_token)=_swap.unwrap();
         let _swap_token_erc20: Ctoken = FromAccountId::from_account_id(*_swap_token);
         let  _origin_token_erc20:Erc20 = FromAccountId::from_account_id(*_origin_token);

         let _swap_token_supply= _swap_token_erc20.total_supply();
         let _orgin_token_supply= _origin_token_erc20.total_supply();
         let price =self.cal_price(_swap_token_supply.into(), _orgin_token_supply.into());
           Ok((_swap_token_supply,_orgin_token_supply,price))
        }

         #[ink(message)]
        pub fn query_publish_token(&self,name:String,cash:Balance)->Result<u128,Error>{
            let _swap= self.get_token_swap_or_default(name);
    
            if cash==0 {
                return Err(Error::InsufficientBalance);
            }
            if let Err(a)=_swap{
               return Err(Error::InsufficientExitSwap);
            };
            let (ref _swap_token,ref _origin_token)=_swap.unwrap();
            let _swap_token_erc20: Ctoken = FromAccountId::from_account_id(*_swap_token);
            let  _origin_token_erc20:Erc20 = FromAccountId::from_account_id(*_origin_token);
   
            let _swap_token_supply= _swap_token_erc20.total_supply();
            let _orgin_token_supply= _origin_token_erc20.total_supply();
           if 0==_swap_token_supply{
               return  Ok(cash );
            };
            let price =self.cal_price(_swap_token_supply.into() , _orgin_token_supply.into());
         
            Ok(price.saturating_mul(cash).wrapping_div(1000u128))
        }

    
        #[ink(message)]
        pub fn query_caller(&self)->AccountId{
            self.admin
        }


        #[ink(message)]
        pub fn query_intrest(&self,name: String )->u32{
            *self.interest.get(&name).unwrap_or(&0u32)
        }


        #[ink(message)]
        pub fn query_rewards(&self,name: String )->Balance{
            *self.rewards.get(&name).unwrap_or(&0u128)
        }

        #[ink(message)]
        pub fn query_interval(&self,name: String )->u32{
            *self.interval.get(&name).unwrap_or(&0u32)
        }


        #[ink(message)]
        pub fn query_swap_token(&self,name:String)->AccountId{
            let  default_account= ink_env::AccountId::from([0x01; 32]);
            *self.swap_token.get(&name).unwrap_or(&default_account)
        }
         /// Returns the owner given the hash or the default address.
        #[ink(message)]
       pub  fn get_token_swap_or_default(&self, name: String) ->Result<(AccountId,AccountId),Error> {
        let mut res:(AccountId,AccountId);
        let _swaps :Option<&(AccountId,AccountId)>= self.swaps.get(&name);
        match _swaps{
            Some(_)=>{
                let (ref swap_token, ref token_id) = _swaps.unwrap();
                res=(*swap_token,*token_id);
            }
            None=>{
                return Err(Error::InsufficientExitSwap)
            //     let bytes: [u8; 32] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            //  res=(AccountId::from(bytes),AccountId::from(bytes));
            }
        }
          Ok(res)
        }

        fn insert_swap(&mut self,name:String,swap_token: AccountId,token_id: AccountId){
            self.swaps.insert(name,(swap_token,token_id));
        }

        #[ink(message)]
        pub fn destory_token(&mut self, amount:Balance, name:String)->Result<(),Error>{
            let caller=Self::env().caller();
               self.only_owner(caller)?;
               if(amount <0 ){
                return Err(Error::InsufficientBalance)
            }
             let _contratc=  Self::env().account_id();
           ///查询融资池
          let _swap= self.get_token_swap_or_default(name);
            if let Err(a)=_swap{
            return Err(Error::InsufficientExitSwap)
           }
           let (ref _swap_token ,ref _origin_token):(AccountId,AccountId)=_swap.unwrap();
           let mut _swap_token_erc20: Ctoken = FromAccountId::from_account_id((*_swap_token).clone());
           _swap_token_erc20.burn(_contratc, amount);

            Ok(())
       
        }

       #[ink(message)]
       pub fn  start_token_interest(&mut self,name:String)->Result<(),Error>{
            let current_block = self.env().block_number();
            self.schedule.entry(name).or_insert(current_block);
           Ok(())
       }

       #[ink(message)]
       pub fn query_current_block(&self,name:String) ->BlockNumber{
           *self.schedule.get(&name).unwrap_or(&0u32)
       }

        #[ink(message)]
        pub fn send_interest(&mut self,name:String)->Result<(),Error>{
         let mut default_account= ink_env::AccountId::from([0x01; 32]);
         let mut block_add:u32=1u32;
            let current_block=self.env().block_number();
            let mut all_rewrad:u128=1u128;
            let key =&name;
            // let val 
            // for (key, val) in &self.rewards {
                if !self.interval.contains_key(key) ||!self.interest.contains_key(key)||!self.rewards.contains_key(key){
                    return Err(Error::InsufficientSwapConfig)
                }
                // if(*val ==0){
                //     continue;
                // }
                let _interval =self.interval.get(key).unwrap_or(&0u32);
                 let _interest =self.interest.get(key).unwrap_or(&0u32);
                 let _schedule= self.schedule.get(key).unwrap_or(&0u32);
                 if(*_interval==0|| *_interest==0|| *_schedule==0){
                    return Err(Error::InsufficientSwapCorrelation)
                 }
                 let token_id = self.swap_token.get(key).unwrap_or(&default_account);
                   if(*token_id ==default_account){
                    return Err(Error::InsufficientSwapPool)
                   }
    
                 if( (current_block - *_schedule) >  *_interval){
                    
                     let mut _swap_token_erc20: Ctoken = FromAccountId::from_account_id((*token_id).clone());
                         let total_supply=  _swap_token_erc20.total_supply();
                   
                      let _n= ((current_block - *_schedule)).wrapping_div(*_interval) ;

                      let reward_add =  self.get_rewards(total_supply.into(),*_interest as u128,_n as u128);
                   
                    
                        let remain_rewars= self.rewards.get(key).unwrap_or(&0u128);
                        if(*remain_rewars == 0u128){
                            return Err(Error::InsufficientRemainRewards)
                        }
                        let mut actual_reward= if *remain_rewars - reward_add  > 0{
                            reward_add
                        }else{
                            *remain_rewars
                        };
                
                        self.schedule.insert((*key).clone(),  (*_interval * _n).saturating_add(*_schedule));
                        if let Some(x) = self.rewards.get_mut(key) {
                            *x=(*x).saturating_sub(actual_reward);
                        }
                
                let mut _swap_token: Ctoken = FromAccountId::from_account_id((*token_id).clone());
                _swap_token.refunds(actual_reward);
                

                  };
            // }


          Ok(())
        }


            fn get_rewards(&self,total_supply:u128, _interest :u128, n:u128)->u128{
                total_supply.saturating_mul(_interest).saturating_mul(n as u128).saturating_add(total_supply).wrapping_div(10000u128)
            }

      
        
    }

    /// Unit tests in Rust are normally defined within such a `#[cfg(test)]`
    /// module and test functions are marked with a `#[test]` attribute.
    /// The below code is technically just normal Rust code.
    #[cfg(test)]
    mod tests {

        #[cfg(not(feature = "ink-as-dependency"))]
        use ink_env::{
            debug_println,
            
        };
        /// Imports all the definitions from the outer scope so we can use them here.
        use super::*;
        use ink_lang as ink;
    
       use ink_env::{
            hash::{
                Blake2x256,
                CryptoHash,
                HashOutput,
            },
            Clear,
        };
        /// We test if the default constructor does its job.
        #[test]
        fn default_works() {
            let mut scm_finance = ScmFinance::default();

            assert_eq!(0u128 ==0u128 , true);

            // assert_eq!(scm_finance.get(), false);
            //  scm_finance.update_swaps(String::from("ETH"),100,2000);
            // assert_eq!(scm_finance.interest.get(&String::from("ETH")).unwrap_or(&40u128), &100u128);
            // assert_eq!(scm_finance.rewards.get(&String::from("ETH")).unwrap_or(&40u128), &2000u128);
        }

        /// We test a simple use case of our contract.
        #[test]
        fn it_create_swap_works() {
            let bytes: [u8; 32] = [70, 229, 185, 240, 139, 39, 86, 26, 218, 180, 253, 83, 104, 179, 133, 218, 180, 63, 215, 123, 32, 78, 48, 9, 75, 78, 14, 177, 50, 45, 6, 60];
            let role_codehash = Hash::from(bytes);
            
            // let mut scm_finance = ScmFinance::new(1000,role_codehash);
                
            //        let accounts =ink_env::test::default_accounts::<ink_env::DefaultEnvironment>().expect("Cannot get accounts");
            //   let name= PrefixedValue{
            //     prefix: b"Erc20::Transfer::to",
            //      value: &"ETH",
            //   };
            //   let _name= <Synmbol as scale::Encode >::encode(&Synmbol("ETH"));
              
            //  scm_finance.insert_swap(encoded_into_hash(&_name),accounts.alice,accounts.bob);
            //  assert_eq!(scm_finance.get_token_swap_or_default(encoded_into_hash(&_name)),(accounts.alice,accounts.bob));

        }

        // fn default_accounts(
        // ) -> ink_env::test::DefaultAccounts<ink_env::Environment> {
        //     ink_env::test::default_accounts::<ink_env::Environment>()
        //         .expect("off-chain environment should have been initialized already")
        // }

            // fn encoded_into_hash<T>(entity: &T) -> Hash
            // where
            //     T: scale::Encode,
            // {
            //     let mut result = Hash::clear();
            //     let len_result = result.as_ref().len();
            //     let encoded = entity.encode();
            //     let len_encoded = encoded.len();
            //     if len_encoded <= len_result {
            //         result.as_mut()[..len_encoded].copy_from_slice(&encoded);
            //         return result
            //     }
            //     let mut hash_output =
            //         <<Blake2x256 as HashOutput>::Type as Default>::default();
            //     <Blake2x256 as CryptoHash>::hash(&encoded, &mut hash_output);
            //     let copy_len = core::cmp::min(hash_output.len(), len_result);
            //     result.as_mut()[0..copy_len].copy_from_slice(&hash_output[0..copy_len]);
            //     result
            // }



    //             /// For calculating the event topic hash.
    // struct PrefixedValue<'a, 'b, T> {
    //     pub prefix: &'a [u8],
    //     pub value: &'b T,
    // }

    // impl<X> scale::Encode for PrefixedValue<'_, '_, X>
    // where
    //     X: scale::Encode,
    // {
    //     #[inline]
    //     fn size_hint(&self) -> usize {
    //         self.prefix.size_hint() + self.value.size_hint()
    //     }

    //     #[inline]
    //     fn encode_to<T: scale::Output>(&self, dest: &mut T) {
    //         self.prefix.encode_to(dest);
    //         self.value.encode_to(dest);
    //     }
    // }
    }

    // struct  Synmbol<'a>(&'a str);

    // impl<'a>  scale::Encode for Synmbol<'a>{
    //     #[inline]
    //     fn size_hint(&self) -> usize {
    //         self.0.size_hint() 
    //     }

    //     #[inline]
    //     fn encode_to<T: scale::Output>(&self, dest: &mut T) {
    //        &self.0.as_bytes().encode_to(dest);
    //     }
    // }

}

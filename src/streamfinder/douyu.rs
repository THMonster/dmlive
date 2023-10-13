// refer to https://github.com/SeaHOH/ykdl
use chrono::prelude::*;
use regex::Regex;
use std::collections::HashMap;
use uuid::Uuid;

use crate::dmlerr;

const DOUYU_API1: &'static str = "https://www.douyu.com/betard/";
// const DOUYU_API2: &'static str = "https://open.douyucdn.cn/api/RoomApi/room/";
const DOUYU_API3: &'static str = "https://www.douyu.com/lapi/live/getH5Play/";

fn get_random_name(l: u8) -> String {
    let mut ret = String::new();
    for _ in 0..l {
        let rn = rand::random::<u32>();
        let n1 = 97 + (rn % 26);
        ret.push(char::from_u32(n1).unwrap())
    }
    ret
}

fn get_js_md5() -> String {
    let mut ret = String::new();
    ret.push_str(r#"var CryptoJS=function(t,n){var i=Object.create||function(){function t(){}return function(n){var i;return t.prototype=n,i=new t,t.prototype=null,i}}(),e={},r=e.lib={},o=r.Base=function(){return{extend:function(t){var n=i(this);return t&&n.mixIn(t),n.hasOwnProperty("init")&&this.init!==n.init||(n.init=function(){n.$super.init.apply(this,arguments)}),n.init.prototype=n,n.$super=this,n},create:function(){var t=this.extend();return t.init.apply(t,arguments),t},init:function(){},mixIn:function(t){for(var n in t)t.hasOwnProperty(n)&&(this[n]=t[n]);t.hasOwnProperty("toString")&&(this.toString=t.toString)},clone:function(){return this.init.prototype.extend(this)}}}(),s=r.WordArray=o.extend({init:function(t,i){t=this.words=t||[],i!=n?this.sigBytes=i:this.sigBytes=4*t.length},toString:function(t){return(t||c).stringify(this)},concat:function(t){var n=this.words,i=t.words,e=this.sigBytes,r=t.sigBytes;if(this.clamp(),e%4)for(var o=0;o<r;o++){var s=i[o>>>2]>>>24-o%4*8&255;n[e+o>>>2]|=s<<24-(e+o)%4*8}else for(var o=0;o<r;o+=4)n[e+o>>>2]=i[o>>>2];return this.sigBytes+=r,this},clamp:function(){var n=this.words,i=this.sigBytes;n[i>>>2]&=4294967295<<32-i%4*8,n.length=t.ceil(i/4)},clone:function(){var t=o.clone.call(this);return t.words=this.words.slice(0),t},random:function(n){for(var i,e=[],r=function(n){var n=n,i=987654321,e=4294967295;return function(){i=36969*(65535&i)+(i>>16)&e,n=18e3*(65535&n)+(n>>16)&e;var r=(i<<16)+n&e;return r/=4294967296,r+=.5,r*(t.random()>.5?1:-1)}},o=0;o<n;o+=4){var a=r(4294967296*(i||t.random()));i=987654071*a(),e.push(4294967296*a()|0)}return new s.init(e,n)}}),a=e.enc={},c=a.Hex={stringify:function(t){for(var n=t.words,i=t.sigBytes,e=[],r=0;r<i;r++){var o=n[r>>>2]>>>24-r%4*8&255;e.push((o>>>4).toString(16)),e.push((15&o).toString(16))}return e.join("")},parse:function(t){for(var n=t.length,i=[],e=0;e<n;e+=2)i[e>>>3]|=parseInt(t.substr(e,2),16)<<24-e%8*4;return new s.init(i,n/2)}},u=a.Latin1={stringify:function(t){for(var n=t.words,i=t.sigBytes,e=[],r=0;r<i;r++){var o=n[r>>>2]>>>24-r%4*8&255;e.push(String.fromCharCode(o))}return e.join("")},parse:function(t){for(var n=t.length,i=[],e=0;e<n;e++)i[e>>>2]|=(255&t.charCodeAt(e))<<24-e%4*8;return new s.init(i,n)}},f=a.Utf8={stringify:function(t){try{return decodeURIComponent(escape(u.stringify(t)))}catch(t){throw new Error("Malformed UTF-8 data")}},parse:function(t){return u.parse(unescape(encodeURIComponent(t)))}},h=r.BufferedBlockAlgorithm=o.extend({reset:function(){this._data=new s.init,this._nDataBytes=0},_append:function(t){"string"==typeof t&&(t=f.parse(t)),this._data.concat(t),this._nDataBytes+=t.sigBytes},_process:function(n){var i=this._data,e=i.words,r=i.sigBytes,o=this.blockSize,a=4*o,c=r/a;c=n?t.ceil(c):t.max((0|c)-this._minBufferSize,0);var u=c*o,f=t.min(4*u,r);if(u){for(var h=0;h<u;h+=o)this._doProcessBlock(e,h);var p=e.splice(0,u);i.sigBytes-=f}return new s.init(p,f)},clone:function(){var t=o.clone.call(this);return t._data=this._data.clone(),t},_minBufferSize:0}),p=(r.Hasher=h.extend({cfg:o.extend(),init:function(t){this.cfg=this.cfg.extend(t),this.reset()},reset:function(){h.reset.call(this),this._doReset()},update:function(t){return this._append(t),this._process(),this},finalize:function(t){t&&this._append(t);var n=this._doFinalize();return n},blockSize:16,_createHelper:function(t){return function(n,i){return new t.init(i).finalize(n)}},_createHmacHelper:function(t){return function(n,i){return new p.HMAC.init(t,i).finalize(n)}}}),e.algo={});return e}(Math);"#);
    ret.push_str(r#"!function(r){return function(e){function t(r,e,t,n,o,a,i){var s=r+(e&t|~e&n)+o+i;return(s<<a|s>>>32-a)+e}function n(r,e,t,n,o,a,i){var s=r+(e&n|t&~n)+o+i;return(s<<a|s>>>32-a)+e}function o(r,e,t,n,o,a,i){var s=r+(e^t^n)+o+i;return(s<<a|s>>>32-a)+e}function a(r,e,t,n,o,a,i){var s=r+(t^(e|~n))+o+i;return(s<<a|s>>>32-a)+e}var i=r,s=i.lib,c=s.WordArray,f=s.Hasher,h=i.algo,u=[];!function(){for(var r=0;r<64;r++)u[r]=4294967296*e.abs(e.sin(r+1))|0}();var v=h.MD5=f.extend({_doReset:function(){this._hash=new c.init([1732584193,4023233417,2562383102,271733878])},_doProcessBlock:function(r,e){for(var i=0;i<16;i++){var s=e+i,c=r[s];r[s]=16711935&(c<<8|c>>>24)|4278255360&(c<<24|c>>>8)}var f=this._hash.words,h=r[e+0],v=r[e+1],d=r[e+2],l=r[e+3],_=r[e+4],p=r[e+5],y=r[e+6],D=r[e+7],H=r[e+8],M=r[e+9],g=r[e+10],m=r[e+11],w=r[e+12],x=r[e+13],B=r[e+14],b=r[e+15],j=f[0],k=f[1],q=f[2],z=f[3];j=t(j,k,q,z,h,7,u[0]),z=t(z,j,k,q,v,12,u[1]),q=t(q,z,j,k,d,17,u[2]),k=t(k,q,z,j,l,22,u[3]),j=t(j,k,q,z,_,7,u[4]),z=t(z,j,k,q,p,12,u[5]),q=t(q,z,j,k,y,17,u[6]),k=t(k,q,z,j,D,22,u[7]),j=t(j,k,q,z,H,7,u[8]),z=t(z,j,k,q,M,12,u[9]),q=t(q,z,j,k,g,17,u[10]),k=t(k,q,z,j,m,22,u[11]),j=t(j,k,q,z,w,7,u[12]),z=t(z,j,k,q,x,12,u[13]),q=t(q,z,j,k,B,17,u[14]),k=t(k,q,z,j,b,22,u[15]),j=n(j,k,q,z,v,5,u[16]),z=n(z,j,k,q,y,9,u[17]),q=n(q,z,j,k,m,14,u[18]),k=n(k,q,z,j,h,20,u[19]),j=n(j,k,q,z,p,5,u[20]),z=n(z,j,k,q,g,9,u[21]),q=n(q,z,j,k,b,14,u[22]),k=n(k,q,z,j,_,20,u[23]),j=n(j,k,q,z,M,5,u[24]),z=n(z,j,k,q,B,9,u[25]),q=n(q,z,j,k,l,14,u[26]),k=n(k,q,z,j,H,20,u[27]),j=n(j,k,q,z,x,5,u[28]),z=n(z,j,k,q,d,9,u[29]),q=n(q,z,j,k,D,14,u[30]),k=n(k,q,z,j,w,20,u[31]),j=o(j,k,q,z,p,4,u[32]),z=o(z,j,k,q,H,11,u[33]),q=o(q,z,j,k,m,16,u[34]),k=o(k,q,z,j,B,23,u[35]),j=o(j,k,q,z,v,4,u[36]),z=o(z,j,k,q,_,11,u[37]),q=o(q,z,j,k,D,16,u[38]),k=o(k,q,z,j,g,23,u[39]),j=o(j,k,q,z,x,4,u[40]),z=o(z,j,k,q,h,11,u[41]),q=o(q,z,j,k,l,16,u[42]),k=o(k,q,z,j,y,23,u[43]),j=o(j,k,q,z,M,4,u[44]),z=o(z,j,k,q,w,11,u[45]),q=o(q,z,j,k,b,16,u[46]),k=o(k,q,z,j,d,23,u[47]),j=a(j,k,q,z,h,6,u[48]),z=a(z,j,k,q,D,10,u[49]),q=a(q,z,j,k,B,15,u[50]),k=a(k,q,z,j,p,21,u[51]),j=a(j,k,q,z,w,6,u[52]),z=a(z,j,k,q,l,10,u[53]),q=a(q,z,j,k,g,15,u[54]),k=a(k,q,z,j,v,21,u[55]),j=a(j,k,q,z,H,6,u[56]),z=a(z,j,k,q,b,10,u[57]),q=a(q,z,j,k,y,15,u[58]),k=a(k,q,z,j,x,21,u[59]),j=a(j,k,q,z,_,6,u[60]),z=a(z,j,k,q,m,10,u[61]),q=a(q,z,j,k,d,15,u[62]),k=a(k,q,z,j,M,21,u[63]),f[0]=f[0]+j|0,f[1]=f[1]+k|0,f[2]=f[2]+q|0,f[3]=f[3]+z|0},_doFinalize:function(){var r=this._data,t=r.words,n=8*this._nDataBytes,o=8*r.sigBytes;t[o>>>5]|=128<<24-o%32;var a=e.floor(n/4294967296),i=n;t[(o+64>>>9<<4)+15]=16711935&(a<<8|a>>>24)|4278255360&(a<<24|a>>>8),t[(o+64>>>9<<4)+14]=16711935&(i<<8|i>>>24)|4278255360&(i<<24|i>>>8),r.sigBytes=4*(t.length+1),this._process();for(var s=this._hash,c=s.words,f=0;f<4;f++){var h=c[f];c[f]=16711935&(h<<8|h>>>24)|4278255360&(h<<24|h>>>8)}return s},clone:function(){var r=f.clone.call(this);return r._hash=this._hash.clone(),r}});i.MD5=f._createHelper(v),i.HmacMD5=f._createHmacHelper(v)}(Math)}(CryptoJS);"#);
    ret
}

pub struct Douyu {}
impl Douyu {
    pub fn new() -> Self {
        Self {}
    }
    pub async fn get_live(&self, room_url: &str) -> anyhow::Result<HashMap<String, String>> {
        let mut ret = HashMap::new();
        let rid = url::Url::parse(room_url)?
            .path_segments()
            .ok_or_else(|| dmlerr!())?
            .last()
            .ok_or_else(|| dmlerr!())?
            .to_string();
        let debug_messages = get_random_name(8);
        let decrypted_codes = get_random_name(8);
        let resoult = get_random_name(8);
        let m_ub98484234 = get_random_name(8);
        let workflow = get_random_name(8);
        let client = reqwest::Client::new();
        let resp = client
            .get(room_url)
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", "https://www.douyu.com/")
            .send()
            .await?
            .text()
            .await?;
        let re = Regex::new(r"(var vdwdae325w_64we =[\s\S]+?)\s*</script>").unwrap();
        let js_enc = re.captures(&resp).ok_or_else(|| dmlerr!())?[1].to_string();
        // let re = Regex::new(r"function ub98484234\(.+?\Weval\((\w+)\);").unwrap();
        // let workflow = re.captures(&js_enc).ok_or("regex err 2")?[1].to_string();
        let js_dom = format!(
            r#"
            {0} = {{{1}: []}};
            if (!this.window) {{window = {{}};}}
            if (!this.document) {{document = {{}};}}
            "#,
            &debug_messages, &decrypted_codes
        );
        let js_patch = format!(
            r#"
            {0}.{1}.push({2});
            var patchCode = function(workflow) {{
                var testVari = /(\w+)=(\w+)\([\w\+]+\);.*?(\w+)="\w+";/.exec(workflow);
                if (testVari && testVari[1] == testVari[2]) {{
                    {2} += testVari[1] + "[" + testVari[3] + "] = function() {{return true;}};";
                }}
            }};
            patchCode({2});
            var subWorkflow = /(?:\w+=)?eval\((\w+)\)/.exec({2});
            if (subWorkflow) {{
                var subPatch = `
                    {0}.{1}.push('sub workflow: ' + subWorkflow);
                    patchCode(subWorkflow);
                `.replace(/subWorkflow/g, subWorkflow[1]) + subWorkflow[0];
                {2} = {2}.replace(subWorkflow[0], subPatch);
            }}
            eval({2});
            "#,
            &debug_messages, &decrypted_codes, &workflow
        );

        let did = Uuid::new_v4().as_simple().encode_lower(&mut Uuid::encode_buffer()).to_string();
        let tsec = format!("{}", Local::now().timestamp());

        let js_debug = format!(
            r#"
            var {2} = ub98484234;
            ub98484234 = function(p1, p2, p3) {{
                try {{
                    var resoult = {2}(p1, p2, p3);
                    {0}.{1} = resoult;
                }} catch(e) {{
                    {0}.{1} = e.message;
                }}
                return {0}.{1};
            }};
            let tmp = {2}("{3}", "{4}", {5});
            console.log(tmp);"#,
            &debug_messages, &resoult, &m_ub98484234, &rid, &did, &tsec
        );
        let js_enc = js_enc.replace(format!("eval({});", workflow).as_str(), &js_patch);
        let js_all = format!("{}{}{}{}", &get_js_md5(), &js_dom, &js_enc, &js_debug);

        let rest1 = crate::utils::js_call(&js_all).await?;
        let rest1 = rest1.get(0).ok_or_else(|| dmlerr!())?;
        let mut param1 = Vec::new();
        let re = Regex::new(r"v=(\d+)").unwrap();
        param1.push((
            "v",
            re.captures(rest1).ok_or_else(|| dmlerr!())?[1].to_string(),
        ));
        let re = Regex::new(r"sign=(\w{32})").unwrap();
        param1.push((
            "sign",
            re.captures(rest1).ok_or_else(|| dmlerr!())?[1].to_string(),
        ));
        param1.push(("did", did));
        param1.push(("tt", tsec));
        param1.push(("cdn", "".to_string()));
        param1.push(("iar", "0".to_string()));
        param1.push(("ive", "0".to_string()));
        param1.push(("rate", "0".to_string()));
        // println!("{:?}", &param1);

        let resp = client
            .post(format!("{}{}", DOUYU_API3, &rid))
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", "https://www.douyu.com/")
            .form(&param1)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        // println!("{:?}", &resp);
        ret.insert(
            String::from("url"),
            format!(
                "{}/{}",
                resp.pointer("/data/rtmp_url").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?,
                resp.pointer("/data/rtmp_live").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?
            ),
        );
        let resp = client
            .get(format!("{}{}", DOUYU_API1, &rid))
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", "https://www.douyu.com/")
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        ret.insert(
            String::from("title"),
            format!(
                "{} - {}",
                resp.pointer("/room/room_name").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?,
                resp.pointer("/room/nickname").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?
            ),
        );

        Ok(ret)
    }
}

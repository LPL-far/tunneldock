const states: Record<string, [string, string]> = {
  draft: ['Draft / authorization pending', '草案／等待授权'], active: ['Active protocol', '协议已启用'],
  awaiting_web_review: ['Awaiting Web review', '等待 Web 审核'], reviewed: ['Web reviewed', 'Web 已审核'],
  queued: ['Queued', '排队中'], running: ['Running', '执行中'], submission_uncertain: ['Submission uncertain', '提交结果不确定'],
  execution_success: ['Execution succeeded', '执行成功'], terminal_unconfirmed: ['Terminal state unconfirmed', '终态未确认'],
  operational_failure: ['Operational failure', '运行失败'], not_submitted: ['Not submitted', '未提交'],
  pending: ['Evidence pending', '证据待校验'], evidence_verified: ['Evidence verified', '证据已校验'],
  evidence_invalid: ['Invalid evidence', '证据无效'], not_available: ['Evidence unavailable', '证据不可用'],
  user: ['User', '用户'], chatgpt: ['ChatGPT Web', 'ChatGPT 网页端'], worker: ['Worker', '执行代理'],
  dispatcher: ['Dispatcher', '调度器'], none: ['None', '无'],
};
const actions: Record<string, [string, string]> = {
  review_recorded: ['Review recorded', '审核已记录'], authorize: ['Review draft and authorize exact hash', '核对草案并授权精确哈希'],
  review_evidence: ['Review evidence and limitations', '审核证据与局限'], room_disabled: ['Enable the Project Room to schedule', '启用项目房间后才能调度'],
  resume: ['Resume scheduling when ready; active work drains', '确认后恢复调度；运行任务继续收尾'],
  resolve_uncertain: ['Inspect uncertain submission; never replay', '检查不确定提交；禁止重放'],
  execute_stage: ['Execute only the approved stage', '仅执行已批准阶段'], queue_stage: ['Wait for approved stage dispatch', '等待批准阶段派发'],
};
export const campaignStateLabel = (key: string, locale: string) => states[key]?.[locale.startsWith('zh') ? 1 : 0] ?? key;
export const campaignActionLabel = (key: string, locale: string) => actions[key]?.[locale.startsWith('zh') ? 1 : 0] ?? key;

const copy: Record<string, string> = {
  'Research campaigns': '研究计划',
  'Protocol verification and scientific gate results remain separate from Web acceptance.': '协议校验、科学门槛结果与 Web 接受分别记录。',
  'Waiting for campaign status.': '等待计划状态。',
  'No recorded campaigns.': '暂无研究计划记录。',
  'Load full status on demand': '按需读取完整状态',
  'Saved. Status updates with Project Room polling.': '已保存，状态将随项目轮询更新。',
  'Plan hash changed; refresh before authorizing.': '计划哈希已变化，请刷新后再授权。',
  'Execution': '执行状态', 'Evidence': '证据状态', 'Gate': '定量门槛',
  'Web review': 'Web 审核', 'Attempts / budget': '尝试次数／预算', 'Deadline': '调度截止时间',
  'not submitted': '未提交', 'not checked': '未校验', 'unknown': '未知',
  'reviewed / accepted': '已审核／已接受', 'reviewed / not accepted': '已审核／未接受', 'pending / not accepted': '待审核／未接受',
  'Active or uncertain submission is draining. Pause/deadline does not terminate processes.': '存在运行中或提交结果不确定的任务。暂停和截止时间不会终止进程。',
  'Recorded scientific failures:': '已记录科学失败：',
  'A later diagnostic does not erase them.': '后续诊断不会抹去这些结果。',
  'Next:': '下一步：', 'Plan and evidence identity': '计划与证据标识',
  'Review draft for authorization': '查看草案并准备授权', 'Inspect frozen plan': '查看冻结计划',
  'Resume scheduling': '恢复调度', 'Pause scheduling': '暂停调度',
  'User authorization reference': '用户授权依据',
  'Reference to your approval of this exact plan': '填写你对此精确计划的授权依据',
  'Authorize this exact plan and enable bounded scheduling': '授权此精确计划并启用有界调度',
  'Create draft from approved-plan JSON': '从计划 JSON 创建草案',
  'Use the version 1 plan schema in docs/AUTO_RESEARCH_CAMPAIGNS.md. A draft cannot dispatch.': '使用 docs/AUTO_RESEARCH_CAMPAIGNS.md 中的版本 1 格式。草案不能派发任务。',
  'Plan JSON': '计划 JSON', 'Save draft': '保存草案',
  ' · paused': ' · 已暂停', ' · room disabled': ' · 房间已禁用',
};
export const campaignCopy = (text: string, locale: string) => locale.startsWith('zh') ? copy[text] ?? text : text;

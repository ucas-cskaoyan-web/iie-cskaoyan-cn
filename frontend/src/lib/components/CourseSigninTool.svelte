<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import QRCode from 'qrcode';

  type Course = {
    id: string;
    uuid: string;
    courseName: string;
    teacherName: string;
    weekDay: string;
    classBeginTime: string;
    classEndTime: string;
    signStatus: string;
  };

  type Source = { mode: 'query'; course: Course } | { mode: 'manual'; identifier: string };
  type StatusKind = 'idle' | 'loading' | 'success' | 'error' | 'info';

  const apiPrefix = '/__article-tools/ucas-course-sign-in';
  const signBase = 'https://iclass.ucas.edu.cn:8181/app/course/stu_scan_sign.action';
  const signBuffer = 3000;
  const qrTtl = 5000;

  let username = '';
  let password = 'Ucas@2025';
  let date = new Date().toISOString().slice(0, 10);
  let keyword = '';
  let manualIdentifier = '';
  let mode: 'query' | 'manual' = 'query';
  let courses: Course[] = [];
  let selectedCourse: Course | null = null;
  let source: Source | null = null;
  let signUrl = '';
  let qrDataUrl = '';
  let expireAt = 0;
  let countdown = 0;
  let offset = 0;
  let offsetFetchedAt = 0;
  let statusText = '输入学号、密码和日期，开始查询课程';
  let statusKind: StatusKind = 'idle';
  let actionText = '生成签到码后，可在此查看下载、复制和点击签到的状态信息';
  let actionKind: StatusKind = 'idle';
  let loading = false;
  let manualLoading = false;
  let directSigning = false;
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;
  let countdownTimer: ReturnType<typeof setInterval> | undefined;
  let filteredCourses: Course[] = [];

  $: filteredCourses = courses.filter((course) => {
    const word = keyword.trim().toLowerCase();
    return !word || course.courseName.toLowerCase().includes(word) || course.teacherName.toLowerCase().includes(word);
  });

  function setStatus(kind: StatusKind, text: string) {
    statusKind = kind;
    statusText = text;
  }

  function setAction(kind: StatusKind, text: string) {
    actionKind = kind;
    actionText = text;
  }

  function api(path: string) {
    return `${apiPrefix}${path}`;
  }

  function formatRange(start: string, end: string) {
    const onlyTime = (value: string) => value?.match(/(\d{2}:\d{2}(?::\d{2})?)$/)?.[1] ?? value ?? '--';
    return `${onlyTime(start)} ~ ${onlyTime(end)}`;
  }

  function buildSignUrl(courseId: string, timestamp: number) {
    return `${signBase}?courseSchedId=${encodeURIComponent(courseId)}&timestamp=${timestamp}`;
  }

  function buildManualUrl(identifier: string, timestamp: number) {
    const raw = identifier.trim();
    if (/^\d+$/.test(raw)) return buildSignUrl(raw, timestamp);
    const uuid = raw.replace(/-/g, '');
    if (/^[0-9a-fA-F]{32}$/.test(uuid)) return `${signBase}?timeTableId=${uuid.toUpperCase()}&timestamp=${timestamp}`;
    return '';
  }

  async function getServerOffset() {
    if (Date.now() - offsetFetchedAt < 30000) return offset;
    try {
      const started = Date.now();
      const response = await fetch(api('/api/course-uuid/timestamp'), { cache: 'no-store' });
      const payload = await response.json();
      if (response.ok && payload.success && typeof payload.timestamp === 'number') {
        offset = payload.timestamp + Math.floor((Date.now() - started) / 2) - Date.now();
        offsetFetchedAt = Date.now();
      }
    } catch {}
    return offset;
  }

  async function refreshQr() {
    if (!source) return;
    const serverNow = Date.now() + await getServerOffset();
    const timestamp = serverNow - signBuffer;
    const url = source.mode === 'query' ? buildSignUrl(source.course.id, timestamp) : buildManualUrl(source.identifier, timestamp);
    if (!url) return;
    signUrl = url;
    qrDataUrl = await QRCode.toDataURL(url, { width: 280, margin: 1, errorCorrectionLevel: 'M' });
    expireAt = serverNow + qrTtl;
    countdown = 5;
    setAction('success', '签到码已生成，约 5 秒后自动刷新');
    if (refreshTimer) clearTimeout(refreshTimer);
    refreshTimer = setTimeout(() => void refreshQr(), qrTtl);
  }

  function updateCountdown() {
    countdown = Math.max(0, Math.ceil((expireAt - (Date.now() + offset)) / 1000));
  }

  async function queryCourses(event: SubmitEvent) {
    event.preventDefault();
    loading = true;
    courses = [];
    selectedCourse = null;
    source = null;
    qrDataUrl = '';
    setStatus('loading', '正在查询课程…');
    try {
      const response = await fetch(api('/api/course-uuid/query'), {
        method: 'POST', headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ username: username.trim(), password, date: date.replace(/-/g, '') })
      });
      const payload = await response.json();
      if (!response.ok) throw new Error(payload.message || '查询失败，请重试');
      courses = payload.courses ?? [];
      setStatus('success', `已查询到 ${payload.total ?? courses.length} 门课程（${payload.date ?? date}）`);
    } catch (error) {
      setStatus('error', error instanceof Error ? error.message : '网络异常，请稍后重试');
    } finally {
      loading = false;
    }
  }

  async function chooseCourse(course: Course) {
    selectedCourse = course;
    source = { mode: 'query', course };
    await refreshQr();
  }

  async function generateManual(event: SubmitEvent) {
    event.preventDefault();
    const url = buildManualUrl(manualIdentifier, Date.now());
    if (!url) {
      setStatus('error', '请输入纯数字课程 ID 或 32 位 UUID');
      return;
    }
    manualLoading = true;
    selectedCourse = null;
    source = { mode: 'manual', identifier: manualIdentifier };
    try {
      await refreshQr();
      setStatus('success', '签到码已生成，约 5 秒后自动刷新');
    } finally {
      manualLoading = false;
    }
  }

  async function directSign() {
    if (!selectedCourse || !username.trim() || !password) {
      setAction('error', '请先在查询课程模式输入学号、密码并选择课程');
      return;
    }
    directSigning = true;
    setAction('loading', '正在发起签到…');
    try {
      const timestamp = Number(new URL(signUrl).searchParams.get('timestamp')) || Date.now() + offset - signBuffer;
      const response = await fetch(api('/api/course-uuid/sign'), {
        method: 'POST', headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ username: username.trim(), password, courseSchedId: selectedCourse.id, timestamp })
      });
      const payload = await response.json();
      if (!response.ok || !payload.success) throw new Error(payload.message || '签到失败，请稍后重试');
      setAction('success', `${payload.message || '签到成功'}，课程状态已刷新`);
      selectedCourse = { ...selectedCourse, signStatus: '1' };
    } catch (error) {
      setAction('error', error instanceof Error ? error.message : '网络异常，签到请求未完成');
    } finally {
      directSigning = false;
    }
  }

  async function downloadQr() {
    if (!source) return;
    const deadline = Date.now() + offset + 10000;
    const url = source.mode === 'query' ? buildSignUrl(source.course.id, deadline) : buildManualUrl(source.identifier, deadline);
    if (!url) return;
    const image = await QRCode.toDataURL(url, { width: 320, margin: 1, errorCorrectionLevel: 'M' });
    const link = document.createElement('a');
    link.href = image;
    link.download = `ucas-signin-${deadline}.png`;
    link.click();
    setAction('success', '二维码已开始下载（10 秒有效）');
  }

  async function copySignUrl() {
    if (!signUrl) return;
    try {
      await navigator.clipboard.writeText(signUrl);
      setAction('info', '已复制签到链接');
    } catch {
      setAction('error', '复制失败，请手动复制签到链接');
    }
  }

  onMount(() => {
    void getServerOffset();
    countdownTimer = setInterval(updateCountdown, 250);
  });

  onDestroy(() => {
    if (refreshTimer) clearTimeout(refreshTimer);
    if (countdownTimer) clearInterval(countdownTimer);
  });
</script>

<section class="course-tool" aria-label="UCAS 课程查询与签到工具">
  <div class="course-tool-tabs">
    <button class:active={mode === 'query'} type="button" onclick={() => mode = 'query'}>查询课程</button>
    <button class:active={mode === 'manual'} type="button" onclick={() => mode = 'manual'}>手动生成</button>
  </div>

  {#if mode === 'query'}
    <form class="course-tool-form" onsubmit={queryCourses}>
      <label>学号<input bind:value={username} required autocomplete="username" placeholder="输入学号" /></label>
      <label>密码<input bind:value={password} required type="password" autocomplete="current-password" placeholder="输入密码" /></label>
      <label>日期<input bind:value={date} required type="date" /></label>
      <button class="primary" type="submit" disabled={loading}>{loading ? '查询中…' : '查询课程'}</button>
    </form>
    <p class:success={statusKind === 'success'} class:error={statusKind === 'error'} class="tool-status">{statusText}</p>
    <div class="course-tool-list-head"><h3>选择课程</h3><input bind:value={keyword} placeholder="筛选课程或教师" aria-label="筛选课程" /></div>
    {#if filteredCourses.length}
      <div class="course-list">
        {#each filteredCourses as course}
          <button class:selected={selectedCourse?.uuid === course.uuid} class="course-row" type="button" onclick={() => chooseCourse(course)}>
            <span><strong>{course.courseName}</strong><small>{course.teacherName} · {formatRange(course.classBeginTime, course.classEndTime)}</small></span>
            <em>{course.signStatus === '1' ? '已签到' : '未签到'}</em>
          </button>
        {/each}
      </div>
    {:else}
      <p class="empty-course">查询后将在这里显示课程。</p>
    {/if}
  {:else}
    <form class="course-tool-form manual" onsubmit={generateManual}>
      <label>课程 ID / UUID<input bind:value={manualIdentifier} required placeholder="7 位课程 ID 或 32 位 UUID" /></label>
      <button class="primary" type="submit" disabled={manualLoading}>{manualLoading ? '生成中…' : '生成签到码'}</button>
    </form>
    <p class:error={statusKind === 'error'} class="tool-status">{statusText}</p>
  {/if}

  {#if qrDataUrl}
    <section class="qr-panel" aria-label="签到码">
      <div><img src={qrDataUrl} alt="签到二维码" /><p>二维码剩余 {countdown} 秒</p></div>
      <div class="qr-actions"><button type="button" onclick={downloadQr}>下载二维码</button><button type="button" onclick={copySignUrl}>复制签到链接</button>{#if selectedCourse}<button class="primary" type="button" onclick={directSign} disabled={directSigning}>{directSigning ? '签到中…' : '直接签到'}</button>{/if}</div>
      <p class:success={actionKind === 'success'} class:error={actionKind === 'error'} class="tool-status">{actionText}</p>
    </section>
  {/if}
</section>

<style>
  .course-tool { margin: 0 0 28px; padding: 18px; border: 1px solid var(--line); border-radius: 12px; background: #f7faf8; }
  .course-tool-tabs { display: flex; margin-bottom: 16px; gap: 8px; }
  .course-tool-tabs button, .qr-actions button { padding: 8px 12px; border: 1px solid var(--line); border-radius: 7px; background: white; color: var(--ink); cursor: pointer; font: inherit; font-size: 12px; }
  .course-tool-tabs button.active, .course-tool-tabs button:hover { border-color: var(--accent); background: var(--accent-soft); color: var(--accent-dark); }
  .course-tool-form { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); align-items: end; gap: 12px; }
  .course-tool-form.manual { grid-template-columns: minmax(0, 1fr) auto; }
  .course-tool-form label { display: grid; color: var(--ink); font-size: 12px; font-weight: 700; gap: 6px; }
  .course-tool-form input, .course-tool-list-head input { min-width: 0; height: 40px; padding: 0 10px; border: 1px solid var(--line); border-radius: 7px; background: white; color: var(--ink); font: inherit; }
  .course-tool button.primary { min-height: 40px; padding: 0 15px; border: 0; border-radius: 7px; background: var(--accent); color: white; cursor: pointer; font: inherit; font-weight: 750; }
  .course-tool button:disabled { cursor: wait; opacity: .6; }
  .tool-status { min-height: 22px; margin: 12px 0; color: var(--muted); font-size: 12px; }
  .tool-status.success { color: #18734d; } .tool-status.error { color: #ad3c35; }
  .course-tool-list-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .course-tool-list-head h3 { margin: 12px 0; color: var(--ink); font-size: 16px; }
  .course-tool-list-head input { width: 230px; height: 34px; font-size: 12px; }
  .course-list { display: grid; border-top: 1px solid var(--line); }
  .course-row { display: flex; padding: 12px 3px; align-items: center; justify-content: space-between; border: 0; border-bottom: 1px solid var(--line); background: transparent; color: var(--ink); cursor: pointer; text-align: left; }
  .course-row:hover, .course-row.selected { background: #eaf4ee; }
  .course-row span { display: grid; gap: 4px; } .course-row strong { font-size: 13px; } .course-row small { color: var(--muted); font-size: 11px; }
  .course-row em { color: var(--muted); font-size: 11px; font-style: normal; }
  .empty-course { margin: 0 0 12px; color: var(--muted); font-size: 12px; }
  .qr-panel { display: grid; margin-top: 18px; padding-top: 18px; grid-template-columns: 190px 1fr; align-items: center; border-top: 1px solid var(--line); gap: 18px; }
  .qr-panel img { display: block; width: 180px; height: 180px; border-radius: 8px; background: white; } .qr-panel p { margin: 6px 0 0; color: var(--muted); font-size: 11px; }
  .qr-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  @media (max-width: 700px) { .course-tool { padding: 12px; } .course-tool-form, .course-tool-form.manual { grid-template-columns: 1fr; } .course-tool-list-head { align-items: stretch; flex-direction: column; } .course-tool-list-head input { width: 100%; } .qr-panel { grid-template-columns: 1fr; } }
</style>

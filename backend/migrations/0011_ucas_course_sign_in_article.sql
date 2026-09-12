INSERT INTO articles (
    id, slug, title, excerpt, body_markdown, category, year, status, is_pinned, published_at
)
VALUES (
    'b28b4af3-8ad4-44c8-8a8e-5b4d8e48dce4',
    'ucas-course-sign-in',
    'UCAS 课程查询与签到工具',
    '在文章中直接查询课程、生成签到二维码，并在签到时间内发起签到。',
    '## 使用说明

本页提供 UCAS 课程查询与签到工具。输入学号、密码和日期后，可以查询当天课程，选择课程生成实时刷新签到二维码，也可以在允许的时间内直接发起签到。

> 请确认你有权使用相关账号和课程签到功能。工具不会在本站文章系统中保存你的学号或密码；使用前请阅读项目说明和学校相关规定。

## 功能边界

- 课程数据和签到结果来自 UCAS 课程系统，本站不保证上游服务始终可用。
- 二维码具有有效期，会自动刷新；请以页面显示的状态为准。
- 涉及账号安全、课程考勤或学校规定的问题，请以学校和课程教师的正式通知为准。',
    'policy',
    2026,
    'published',
    false,
    now()
)
ON CONFLICT (slug) DO NOTHING;

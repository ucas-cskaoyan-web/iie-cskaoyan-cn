import type { NextConfig } from "next";

const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? "/__article-tools/ucas-course-sign-in";

const nextConfig: NextConfig = {
  basePath,
};

export default nextConfig;

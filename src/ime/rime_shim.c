/* Compile against librime's versioned API table instead of mirroring its layout in Rust. */
#include <rime_api.h>
#include <stdlib.h>
#include <string.h>
static RimeApi *api;
static const char *runtime_modules[] = {"default", NULL};
static const char *deployment_modules[] = {"default", "deployer", NULL};
int nk_rime_init(const char *user, const char *cache, int deploy) {
    api = rime_get_api();
    if (!RIME_API_AVAILABLE(api, select_candidate_on_current_page)) return 0;
    RIME_STRUCT(RimeTraits, traits);
    traits.shared_data_dir = "/usr/share/rime-data";
    traits.user_data_dir = user;
    traits.staging_dir = cache;
    traits.app_name = "rime.novakeys";
    traits.modules = deploy ? deployment_modules : runtime_modules;
    traits.min_log_level = 2;
    traits.log_dir = "";
    api->setup(&traits);
    api->initialize(&traits);
    if (deploy) {
        api->deployer_initialize(&traits);
        if (api->start_maintenance(True)) api->join_maintenance_thread();
    }
    const char *schemas[] = {"luna_pinyin_simp", "luna_pinyin"};
    for (int i = 0; i < 2; ++i) {
        RimeConfig config = {0};
        if (!api->schema_open(schemas[i], &config)) { api->finalize(); return 0; }
        Bool user_dict = True, custom_phrase = True;
        int valid = api->config_get_bool(&config, "translator/enable_user_dict", &user_dict)
            && api->config_get_bool(&config, "custom_phrase/enable_user_dict", &custom_phrase)
            && !user_dict && !custom_phrase;
        api->config_close(&config);
        if (!valid) { api->finalize(); return 0; }
    }
    return 1;
}
void nk_rime_finalize(void) { api->finalize(); }
uintptr_t nk_rime_new(int traditional) {
    RimeSessionId session = api->create_session();
    if (!session) return 0;
    if (!api->select_schema(session, traditional ? "luna_pinyin" : "luna_pinyin_simp")) { api->destroy_session(session); return 0; }
    api->set_option(session, "ascii_mode", False);
    return session;
}
void nk_rime_delete(uintptr_t session) { api->destroy_session(session); }
int nk_rime_key(uintptr_t session, int key) { return api->process_key(session, key, 0); }
int nk_rime_select(uintptr_t session, unsigned index) { return api->select_candidate_on_current_page(session, index); }
int nk_rime_finish(uintptr_t session) { return api->commit_composition(session); }
void nk_rime_clear(uintptr_t session) { api->clear_composition(session); }
char *nk_rime_commit(uintptr_t session) {
    RIME_STRUCT(RimeCommit, commit);
    if (!api->get_commit(session, &commit)) return NULL;
    char *text = commit.text ? strdup(commit.text) : NULL;
    api->free_commit(&commit);
    return text;
}
void nk_rime_free_text(char *text) { free(text); }
RimeContext *nk_rime_snapshot(uintptr_t session) {
    RimeContext *context = calloc(1, sizeof(RimeContext));
    if (!context) return NULL;
    RIME_STRUCT_INIT(RimeContext, *context);
    if (!api->get_context(session, context)) { free(context); return NULL; }
    return context;
}
void nk_rime_free_snapshot(RimeContext *context) { api->free_context(context); free(context); }
const char *nk_rime_preedit(RimeContext *context) { return context->composition.preedit; }
const char *nk_rime_candidate(RimeContext *context, unsigned index) {
    return index < (unsigned)context->menu.num_candidates ? context->menu.candidates[index].text : NULL;
}
void nk_rime_info(RimeContext *context, int *count, int *page, int *last, int *selected, int *cursor, int *start, int *end) {
    *count = context->menu.num_candidates; *page = context->menu.page_no;
    *last = context->menu.is_last_page; *selected = context->menu.highlighted_candidate_index;
    *cursor = context->composition.cursor_pos; *start = context->composition.sel_start; *end = context->composition.sel_end;
}

const char *nk_rime_version(void) { RimeApi *value=rime_get_api(); return RIME_API_AVAILABLE(value,get_version) ? value->get_version() : "unknown"; }
